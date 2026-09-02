use std::{
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use garive_desktop::{
    builtin_management_validator, BuiltinDesktopProfileRegistry, DesktopConfigurationProvider,
    DesktopHost, DesktopSecretResolver, DesktopSetupInput, DesktopSetupService,
    DesktopWorkspaceService, FileDesktopConfigurationProvider, SystemDesktopSecretResolver,
    SystemDesktopWorkspaceBookmarkStore, SystemSetupCredentialStore, DESKTOP_CONFIG_FILE,
    DESKTOP_WORKSPACE_MANIFEST_FILE,
};
use garive_provider_profile::SecretValue;
use garive_runtime::{
    LiveHostServer, ManagementCommitBody, ManagementConfigStore, SqliteLedger, SqliteLedgerError,
};
use serde_json::json;

const DEFAULT_LISTEN: &str = "127.0.0.1:8787";

#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("garive-host: {message}");
        std::process::exit(2);
    }
}

async fn run() -> Result<(), &'static str> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, directory] if command == "serve" => {
            serve(PathBuf::from(directory), DEFAULT_LISTEN).await
        }
        [command, directory, listen] if command == "serve" => {
            serve(PathBuf::from(directory), listen).await
        }
        [command, directory] if command == "serve-stdin" => {
            serve_from_stdin(PathBuf::from(directory), DEFAULT_LISTEN).await
        }
        [command, directory, listen] if command == "serve-stdin" => {
            serve_from_stdin(PathBuf::from(directory), listen).await
        }
        [command, directory, profile, endpoint, target, model, definition]
            if command == "configure" =>
        {
            configure(
                Path::new(directory),
                profile,
                endpoint,
                target,
                model,
                definition,
            )
        }
        [command, directory, profile, endpoint, target, model, definition, deployment, runtime]
            if command == "setup-management" =>
        {
            setup_management(
                Path::new(directory),
                profile,
                endpoint,
                target,
                model,
                definition,
                deployment,
                runtime,
            )
        }
        [command, directory] if command == "clear-management" => {
            clear_management(Path::new(directory))
        }
        _ => {
            eprintln!("usage: garive-host serve <config-dir> [127.0.0.1:8787]");
            eprintln!("       garive-host serve-stdin <config-dir> [127.0.0.1:8787]");
            eprintln!("       garive-host configure <config-dir> <profile> <endpoint> <target> <model> <definition>");
            eprintln!("       configure reads the write-only connection credential from stdin");
            eprintln!();
            eprintln!("       garive-host setup-management <config-dir> <profile> <endpoint> <target> <model> <definition> <deployment> <runtime-id>");
            eprintln!(
                "       setup-management reads the write-only connection credential from stdin"
            );
            eprintln!(
                "       writes the singleton runtime_management_config row (loopback SQLite path)"
            );
            eprintln!("       garive-host clear-management <config-dir>");
            Err("invalid_arguments")
        }
    }
}

fn configure(
    directory: &Path,
    profile: &str,
    endpoint: &str,
    target: &str,
    model: &str,
    definition: &str,
) -> Result<(), &'static str> {
    let mut credential = String::new();
    std::io::stdin()
        .take(16_385)
        .read_to_string(&mut credential)
        .map_err(|_| "credential_read_failed")?;
    let credential = credential.trim_end_matches(['\r', '\n']);
    if credential.is_empty() || credential.len() > 16_384 {
        return Err("credential_rejected");
    }
    let setup = DesktopSetupService::new(directory.to_owned(), SystemSetupCredentialStore);
    setup.recover(false).map_err(|_| "setup_recovery_failed")?;
    let catalogue = setup.catalogue();
    let preset = catalogue.presets.first().ok_or("setup_catalogue_invalid")?;
    let plan = setup
        .prepare(DesktopSetupInput {
            schema_version: catalogue.schema_version,
            caller_nonce: format!("host-cli-{}", std::process::id()),
            catalogue_revision: catalogue.catalogue_revision.to_owned(),
            preset_id: preset.preset_id.to_owned(),
            profile_id: profile.to_owned(),
            endpoint_override: Some(endpoint.to_owned()),
            model_target_id: target.to_owned(),
            model_id: model.to_owned(),
            deployment_id: format!("local-{target}"),
            definition_id: definition.to_owned(),
        })
        .map_err(|_| "setup_prepare_failed")?;
    let receipt = setup
        .commit(&plan.plan_digest, credential)
        .map_err(|_| "setup_commit_failed")?;
    println!(
        "configured revision {}; restart required",
        receipt.configuration_revision
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn setup_management(
    directory: &Path,
    profile: &str,
    endpoint: &str,
    target: &str,
    model: &str,
    definition: &str,
    deployment: &str,
    runtime_id: &str,
) -> Result<(), &'static str> {
    let mut credential = String::new();
    std::io::stdin()
        .take(16_385)
        .read_to_string(&mut credential)
        .map_err(|_| "credential_read_failed")?;
    let credential = credential.trim_end_matches(['\r', '\n']).to_owned();
    if credential.is_empty() || credential.len() > 16_384 {
        return Err("credential_rejected");
    }
    let validator = builtin_management_validator();
    let body = ManagementCommitBody {
        schema_version: 1,
        profile_id: profile.to_owned(),
        endpoint_override: Some(endpoint.to_owned()),
        model_target_id: target.to_owned(),
        model_id: model.to_owned(),
        deployment_id: deployment.to_owned(),
        definition_id: definition.to_owned(),
        api_key: credential,
        runtime_id: runtime_id.to_owned(),
    };
    validator
        .validate(&body)
        .map_err(|_| "management_validation_failed")?;
    let database = directory.join("garive-desktop.db");
    if let Some(parent) = database.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "database_directory_unwritable")?;
    }
    let mut ledger = SqliteLedger::open(&database).map_err(map_sqlite_open)?;
    let committed_at = committed_at_now();
    let receipt = ledger
        .management_config_store()
        .commit(&body, &committed_at)
        .map_err(map_commit_error)?;
    println!(
        "{}",
        json!({
            "schema_version": receipt.schema_version,
            "configuration_revision": receipt.configuration_revision,
            "configuration_digest": receipt.configuration_digest,
            "restart_required": receipt.restart_required,
            "receipt_digest": receipt.receipt_digest,
        })
    );
    Ok(())
}

fn clear_management(directory: &Path) -> Result<(), &'static str> {
    let database = directory.join("garive-desktop.db");
    let mut ledger = SqliteLedger::open(&database).map_err(map_sqlite_open)?;
    ledger
        .management_config_store()
        .clear()
        .map_err(|_| "management_clear_failed")?;
    Ok(())
}

fn map_sqlite_open(error: SqliteLedgerError) -> &'static str {
    match error {
        SqliteLedgerError::UnsupportedSchema(_) => "unsupported_schema",
        _ => "database_unavailable",
    }
}

fn committed_at_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    // Hand-rolled RFC 3339 timestamp; the workspace chrono build has only
    // the "std" feature, so we avoid `Utc::now()`.
    let (year, month, day, hour, minute, second) = unix_to_civil(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z",)
}

fn unix_to_civil(seconds: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let secs_of_day = (seconds % 86_400) as u32;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_1970: i64) -> (i32, u32, u32) {
    // Howard Hinnant's algorithm (public domain).
    let z = days_since_1970 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn map_commit_error(error: garive_runtime::ManagementConfigError) -> &'static str {
    use garive_runtime::ManagementConfigError as E;
    match error {
        E::ProfileUnknown => "management_profile_unknown",
        E::DefinitionUnknown => "management_definition_unknown",
        E::EndpointInvalid => "management_endpoint_invalid",
        E::ApiKeyInvalid => "management_api_key_invalid",
        E::RuntimeIdInvalid => "management_runtime_id_invalid",
        E::IdentifierInvalid => "management_identifier_invalid",
        E::SchemaVersionUnsupported => "management_schema_version_unsupported",
        E::StorageFailed => "management_storage_failed",
        E::NotConfigured => "management_not_configured",
    }
}

async fn serve(directory: PathBuf, listen: &str) -> Result<(), &'static str> {
    serve_with_resolver(directory, listen, SystemDesktopSecretResolver).await
}

async fn serve_from_stdin(directory: PathBuf, listen: &str) -> Result<(), &'static str> {
    let credential = read_credential()?;
    serve_with_resolver(directory, listen, SuppliedCredential(credential)).await
}

fn read_credential() -> Result<String, &'static str> {
    let mut credential = String::new();
    std::io::stdin()
        .take(16_385)
        .read_to_string(&mut credential)
        .map_err(|_| "credential_read_failed")?;
    let credential = credential.trim_end_matches(['\r', '\n']).to_owned();
    if credential.is_empty() || credential.len() > 16_384 {
        return Err("credential_rejected");
    }
    Ok(credential)
}

struct SuppliedCredential(String);

impl DesktopSecretResolver for SuppliedCredential {
    fn resolve(
        &self,
        _credential_ref: &str,
    ) -> Result<SecretValue, garive_desktop::DesktopConfigurationError> {
        SecretValue::new(self.0.clone())
            .map_err(|_| garive_desktop::DesktopConfigurationError::SecretUnavailable)
    }
}

async fn serve_with_resolver<R: DesktopSecretResolver>(
    directory: PathBuf,
    listen: &str,
    resolver: R,
) -> Result<(), &'static str> {
    let address: SocketAddr = listen.parse().map_err(|_| "listen_address_invalid")?;
    if !address.ip().is_loopback() {
        return Err("listen_address_not_loopback");
    }
    let provider = FileDesktopConfigurationProvider::new(
        directory.join(DESKTOP_CONFIG_FILE),
        directory.clone(),
        resolver,
        BuiltinDesktopProfileRegistry,
    );
    let config = provider
        .load()
        .map_err(|_| "configuration_load_failed")?
        .ok_or("configuration_missing")?;
    let workspaces = DesktopWorkspaceService::durable(
        directory.join(DESKTOP_WORKSPACE_MANIFEST_FILE),
        Arc::new(SystemDesktopWorkspaceBookmarkStore),
    );
    let _workspace_recovery = workspaces.recover("host-cli");
    let host = Arc::new(
        DesktopHost::new_with_workspaces(config, workspaces, "host-cli")
            .map_err(|_| "host_construction_failed")?,
    );
    let live_host = host
        .live_host()
        .with_management_validator(builtin_management_validator());
    let server = LiveHostServer::bind(live_host, address)
        .await
        .map_err(|_| "host_bind_failed")?;
    println!("Garive Host listening on http://{}", server.local_addr());
    tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move {
                loop {
                    match host.drive_pending().await {
                        Ok(true) => {}
                        Ok(false) => tokio::time::sleep(Duration::from_millis(20)).await,
                        Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                    }
                }
            });
            server
                .serve(std::future::pending())
                .await
                .map_err(|_| "host_serve_failed")
        })
        .await
}

// Suppress dead-code noise if future binary revisions stop using these
// re-exports from the management port (the runtime contract still uses them
// in commit-3 handlers and tests).
#[allow(unused_imports)]
use ManagementConfigStore as _Store;
