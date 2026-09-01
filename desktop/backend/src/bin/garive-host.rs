use std::{
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use garive_desktop::{
    BuiltinDesktopProfileRegistry, DesktopConfigurationProvider, DesktopHost,
    DesktopSecretResolver, DesktopSetupInput, DesktopSetupService, DesktopWorkspaceService,
    FileDesktopConfigurationProvider, SystemDesktopSecretResolver,
    SystemDesktopWorkspaceBookmarkStore, SystemSetupCredentialStore, DESKTOP_CONFIG_FILE,
    DESKTOP_WORKSPACE_MANIFEST_FILE,
};
use garive_provider_profile::SecretValue;
use garive_runtime::LiveHostServer;

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
        _ => {
            eprintln!("usage: garive-host serve <config-dir> [127.0.0.1:8787]");
            eprintln!("       garive-host serve-stdin <config-dir> [127.0.0.1:8787]");
            eprintln!("       garive-host configure <config-dir> <profile> <endpoint> <target> <model> <definition>");
            eprintln!("       configure reads the write-only connection credential from stdin");
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
    let server = LiveHostServer::bind(host.live_host(), address)
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
