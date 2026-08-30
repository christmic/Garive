use tauri::Manager;

#[tauri::command]
async fn run_agent_turn(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    definition_id: String,
    input: String,
) -> Result<garive_desktop::DesktopTurnResult, String> {
    state
        .run_turn_isolated(definition_id, input)
        .await
        .map_err(|error| error.code().to_owned())
}

type SetupState = garive_desktop::DesktopSetupService<garive_desktop::SystemSetupCredentialStore>;

#[tauri::command]
fn get_setup_catalogue(
    setup: tauri::State<'_, SetupState>,
) -> garive_desktop::DesktopSetupCatalogue {
    setup.catalogue()
}

#[tauri::command]
fn prepare_setup(
    setup: tauri::State<'_, SetupState>,
    input: garive_desktop::DesktopSetupInput,
) -> Result<garive_desktop::DesktopSetupPlan, String> {
    setup
        .prepare(input)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn commit_setup(
    setup: tauri::State<'_, SetupState>,
    plan_digest: String,
    credential: String,
) -> Result<garive_desktop::DesktopSetupReceipt, String> {
    setup
        .commit(&plan_digest, &credential)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn restart_desktop(app: tauri::AppHandle) {
    app.restart()
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app
                .path()
                .app_config_dir()
                .map_err(|_| stable_setup_error("config_directory"))?;
            let provider = garive_desktop::FileDesktopConfigurationProvider::new(
                directory.join(garive_desktop::DESKTOP_CONFIG_FILE),
                directory.clone(),
                garive_desktop::SystemDesktopSecretResolver,
                garive_desktop::BuiltinDesktopProfileRegistry,
            );
            let state = garive_desktop::DesktopState::default();
            state
                .install_from(&provider)
                .map_err(|error| stable_setup_error(error.code()))?;
            app.manage(state);
            app.manage(garive_desktop::DesktopSetupService::new(
                directory,
                garive_desktop::SystemSetupCredentialStore,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_setup_catalogue,
            prepare_setup,
            commit_setup,
            restart_desktop,
            run_agent_turn
        ])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}

fn stable_setup_error(code: &'static str) -> std::io::Error {
    std::io::Error::other(code)
}
