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
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use garive_runtime::headless::{
    build_headless_installation, build_headless_model_port, headless_execution_attempt,
    headless_execution_policy, headless_now_ms, HeadlessClock, HeadlessConfiguration,
    HeadlessConstructionError,
};
use garive_runtime::{
    drive_pending, local_dispatch_queue, AllowAllValidator, CatalogueCapabilityPreparationFactory,
    DrivePendingOutcome, HostClock, LiveHost, LiveHostLimits, LiveHostServer, LiveOutputHub,
    LiveOutputLimits, LocalExecutionWorker, SqliteLedger, SqliteLedgerError,
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
    let (directory, listen) = match arguments.as_slice() {
        [directory] => (PathBuf::from(directory), DEFAULT_LISTEN),
        [directory, listen] => (PathBuf::from(directory), listen.as_str()),
        _ => {
            eprintln!("usage: garive-headless <config-dir> [127.0.0.1:8787]");
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
    let (installation, catalogue) =
        build_headless_installation(&configuration).map_err(map_construction_error)?;
    let preparation = Arc::new(CatalogueCapabilityPreparationFactory::new(catalogue, None));
    let policy = headless_execution_policy(&configuration);

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
    let worker = LocalExecutionWorker::new(&database_path, policy, model, preparation)
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
