//! `garive-headless` — drives H1 sessions against the committed
//! `runtime_management_config` SQLite row.
//!
//! Usage:
//!
//! ```text
//! garive-headless <config-dir> [127.0.0.1:8787]
//! ```
//!
//! The `<config-dir>` must contain `garive-desktop.db` with a committed
//! singleton row (use `garive-host setup-management <config-dir> ...` to
//! commit it). The `[listen]` defaults to `127.0.0.1:8787` and is
//! rejected unless it is loopback.

use std::{
    fs::DirBuilder,
    io::Read,
    net::SocketAddr,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use garive_runtime::headless::{
    build_headless_installation, build_headless_model_port, build_headless_workspace_installation,
    headless_execution_attempt, headless_execution_policy, headless_now_ms,
    headless_workspace_execution_policy, HeadlessClock, HeadlessConfiguration,
    HeadlessConstructionError,
};
use garive_runtime::{
    drive_pending, local_dispatch_queue, AllowAllValidator, CatalogueBoundGovernedExecutionFactory,
    CatalogueCapabilityPreparationFactory, DrivePendingOutcome, HeadlessWorkspaceExecutionFactory,
    HostClock, LiveHost, LiveHostLimits, LiveHostServer, LiveOutputHub, LiveOutputLimits,
    LocalExecutionWorker, ManagementCommitBody, SqliteLedger, SqliteLedgerError,
    T1WorkspaceRuntimeConfig, HEADLESS_WORKSPACE_EXECUTOR_REVISION,
    HEADLESS_WORKSPACE_POLICY_REVISION, MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
};

const DEFAULT_LISTEN: &str = "127.0.0.1:8787";

#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("garive-headless: {message}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), String> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.first().map(String::as_str) == Some("setup") {
        return setup(&arguments[1..]);
    }
    let (directory, listen, workspace) = match arguments.as_slice() {
        [directory] => (PathBuf::from(directory), DEFAULT_LISTEN, None),
        [directory, listen] => (PathBuf::from(directory), listen.as_str(), None),
        [directory, listen, workspace] => (
            PathBuf::from(directory),
            listen.as_str(),
            Some(PathBuf::from(workspace)),
        ),
        _ => {
            eprintln!("usage: garive-headless <config-dir> [127.0.0.1:8787] [workspace-root]");
            eprintln!();
            eprintln!("Reads the singleton runtime_management_config row from");
            eprintln!("<config-dir>/garive-desktop.db and serves H1 over loopback.");
            return Err("invalid_arguments".to_owned());
        }
    };

    let address: SocketAddr = listen
        .parse()
        .map_err(|_| "listen_address_invalid".to_owned())?;
    if !address.ip().is_loopback() {
        return Err("listen_address_not_loopback".to_owned());
    }

    let database_path = directory.join("garive-desktop.db");
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "database_directory_unwritable".to_owned())?;
    }

    let configuration = load_headless_configuration(&database_path)?;
    println!(
        "garive-headless: profile={} definition={} runtime_id={} revision={}",
        configuration.state.profile_id,
        configuration.state.definition_id,
        configuration.state.runtime_id,
        configuration.state.configuration_revision,
    );

    let model = build_headless_model_port(&configuration).map_err(map_construction_error)?;
    let workspace_config = workspace
        .map(|root| headless_workspace_config(&directory, root))
        .transpose()?;
    let (installation, catalogue) = match &workspace_config {
        Some(config) => {
            let execution = config
                .build()
                .map_err(|_| "workspace_construction_failed".to_owned())?;
            build_headless_workspace_installation(&configuration, execution.capabilities())
                .map_err(map_construction_error)?
        }
        None => build_headless_installation(&configuration).map_err(map_construction_error)?,
    };
    let preparation = Arc::new(CatalogueCapabilityPreparationFactory::new(
        catalogue.clone(),
        None,
    ));
    let policy = if workspace_config.is_some() {
        headless_workspace_execution_policy(&configuration)
    } else {
        headless_execution_policy(&configuration)
    };

    let clock: Arc<dyn HostClock> = Arc::new(HeadlessClock);
    let limits = LiveHostLimits {
        max_command_bytes: 1024 * 1024,
        event_batch_size: 64,
        event_poll_interval_ms: 100,
        activity: None,
    };

    let hub = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 8,
        max_preview_bytes: 1024 * 1024,
        max_event_bytes: 4096,
        broadcast_capacity: 64,
        max_subscribers_per_session: 8,
    })
    .map_err(|_| "live_output_hub_invalid_limits".to_owned())?;

    let (dispatcher, mut queue) =
        local_dispatch_queue(64).map_err(|_| "queue_construction_failed".to_owned())?;
    let worker = match workspace_config {
        Some(config) => {
            let governed = Arc::new(
                HeadlessWorkspaceExecutionFactory::new(config, "headless-workspace")
                    .map_err(|_| "workspace_construction_failed".to_owned())?,
            );
            LocalExecutionWorker::new_governed(
                &database_path,
                policy,
                model,
                Arc::new(CatalogueBoundGovernedExecutionFactory::new(
                    catalogue.clone(),
                    governed,
                )),
                preparation,
            )
        }
        None => LocalExecutionWorker::new(&database_path, policy, model, preparation),
    }
    .map_err(|_| "worker_construction_failed".to_owned())?
    .with_live_output(hub.clone());

    let host = LiveHost::new_catalogue_with_live_output(
        &database_path,
        vec![installation.clone_installed_agent()],
        limits,
        clock,
        dispatcher,
        hub,
    )
    .map_err(|_| "host_construction_failed".to_owned())?
    .with_management_validator(Arc::new(AllowAllValidator));

    let server = LiveHostServer::bind(host, address)
        .await
        .map_err(|error| format!("host_bind_failed: {error}"))?;
    println!(
        "Garive Host (headless) listening on http://{}",
        server.local_addr()
    );

    tokio::task::LocalSet::new()
        .run_until(async move {
            let worker = Arc::new(worker);
            tokio::task::spawn_local(async move {
                loop {
                    let attempt = headless_execution_attempt(headless_now_ms());
                    match drive_pending(&mut queue, worker.as_ref(), &attempt).await {
                        DrivePendingOutcome::Advanced => {}
                        DrivePendingOutcome::Idle => {
                            tokio::time::sleep(Duration::from_millis(20)).await
                        }
                        DrivePendingOutcome::Stopped => break,
                        DrivePendingOutcome::Failed => {
                            tokio::time::sleep(Duration::from_millis(100)).await
                        }
                    }
                }
            });
            let result = server.serve(std::future::pending()).await;
            match result {
                Ok(()) => Ok(()),
                Err(error) => Err(format!("host_serve_failed: {error}")),
            }
        })
        .await
}

fn headless_workspace_config(
    directory: &Path,
    workspace: PathBuf,
) -> Result<T1WorkspaceRuntimeConfig, String> {
    let recovery = directory.join("headless-patch-recovery");
    if !recovery.exists() {
        DirBuilder::new()
            .mode(0o700)
            .create(&recovery)
            .map_err(|_| "workspace_recovery_unavailable".to_owned())?;
    }
    T1WorkspaceRuntimeConfig::new(
        HEADLESS_WORKSPACE_POLICY_REVISION,
        HEADLESS_WORKSPACE_EXECUTOR_REVISION,
        workspace,
        recovery,
    )
    .map_err(|_| "workspace_construction_failed".to_owned())
}

fn setup(arguments: &[String]) -> Result<(), String> {
    let [directory, profile_id, endpoint, model_target_id, model_id, definition_id, deployment_id, runtime_id] =
        arguments
    else {
        eprintln!(
            "usage: garive-headless setup <config-dir> <profile-id> <endpoint|-> \
             <target-id> <model-id> <definition-id> <deployment-id> <runtime-id>"
        );
        eprintln!("reads the provider credential from stdin");
        return Err("invalid_arguments".to_owned());
    };
    let mut api_key = String::new();
    std::io::stdin()
        .read_to_string(&mut api_key)
        .map_err(|_| "credential_read_failed".to_owned())?;
    let api_key = api_key.trim_end_matches(['\r', '\n']).to_owned();
    let database_path = PathBuf::from(directory).join("garive-desktop.db");
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "database_directory_unwritable".to_owned())?;
    }
    let mut ledger = SqliteLedger::open(&database_path).map_err(map_sqlite_error)?;
    let receipt = ledger
        .management_config_store()
        .commit(
            &ManagementCommitBody {
                schema_version: MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
                profile_id: profile_id.clone(),
                endpoint_override: (endpoint != "-").then(|| endpoint.clone()),
                model_target_id: model_target_id.clone(),
                model_id: model_id.clone(),
                deployment_id: deployment_id.clone(),
                definition_id: definition_id.clone(),
                api_key,
                runtime_id: runtime_id.clone(),
            },
            &chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
        )
        .map_err(|error| error.wire_code().to_owned())?;
    println!(
        "garive-headless: configuration committed revision={} digest={} restart_required={}",
        receipt.configuration_revision, receipt.configuration_digest, receipt.restart_required
    );
    Ok(())
}

fn load_headless_configuration(database_path: &Path) -> Result<HeadlessConfiguration, String> {
    let mut ledger = SqliteLedger::open(database_path).map_err(map_sqlite_error)?;
    let wrapper = ledger
        .management_config_store()
        .read_with_credential()
        .map_err(|_| "management_storage_failed".to_owned())?
        .ok_or_else(|| "management_not_configured".to_owned())?;
    Ok(HeadlessConfiguration {
        state: wrapper.state,
        api_key: wrapper.api_key,
    })
}

fn map_construction_error(error: HeadlessConstructionError) -> String {
    error.code().to_owned()
}

fn map_sqlite_error(error: SqliteLedgerError) -> String {
    match error {
        SqliteLedgerError::UnsupportedSchema(_) => "unsupported_schema".to_owned(),
        _ => "database_unavailable".to_owned(),
    }
}
