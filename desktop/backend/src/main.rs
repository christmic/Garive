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
    window: tauri::Window,
    setup: tauri::State<'_, SetupState>,
) -> Result<garive_desktop::DesktopSetupCatalogue, String> {
    require_setup_window(&window)?;
    Ok(setup.catalogue())
}

#[tauri::command]
fn get_setup_state(
    window: tauri::Window,
    setup: tauri::State<'_, SetupState>,
) -> Result<garive_desktop::DesktopSetupState, String> {
    require_setup_window(&window)?;
    Ok(setup.state())
}

#[tauri::command]
fn prepare_setup(
    window: tauri::Window,
    setup: tauri::State<'_, SetupState>,
    input: garive_desktop::DesktopSetupInput,
) -> Result<garive_desktop::DesktopSetupPlan, String> {
    require_setup_window(&window)?;
    setup
        .prepare(input)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn commit_setup(
    window: tauri::Window,
    setup: tauri::State<'_, SetupState>,
    plan_digest: String,
    credential: garive_desktop::SensitiveSetupCredential,
) -> Result<garive_desktop::DesktopSetupReceipt, String> {
    require_setup_window(&window)?;
    setup
        .commit(&plan_digest, credential.expose_secret())
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn cancel_setup(
    window: tauri::Window,
    setup: tauri::State<'_, SetupState>,
    plan_digest: String,
) -> Result<garive_desktop::DesktopSetupCancellation, String> {
    require_setup_window(&window)?;
    setup
        .cancel(&plan_digest)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn restart_desktop(window: tauri::Window, app: tauri::AppHandle) -> Result<(), String> {
    require_setup_window(&window)?;
    app.restart()
}

fn require_setup_window(window: &tauri::Window) -> Result<(), String> {
    garive_desktop::authorize_setup_window(window.label()).map_err(|error| error.code().to_owned())
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
            let setup = garive_desktop::DesktopSetupService::new(
                directory,
                garive_desktop::SystemSetupCredentialStore,
            );
            let state = garive_desktop::DesktopState::default();
            let recovered = setup.recover(false).is_ok();
            if recovered {
                match state.install_from(&provider) {
                    Ok(installed) => {
                        if setup.recover(installed).is_err() {
                            setup
                                .complete_startup(false, Some("setup_recovery_failed"))
                                .map_err(|error| stable_setup_error(error.code()))?;
                        } else {
                            setup
                                .complete_startup(installed, None)
                                .map_err(|error| stable_setup_error(error.code()))?;
                        }
                    }
                    Err(error) => setup
                        .complete_startup(false, Some(error.code()))
                        .map_err(|setup_error| stable_setup_error(setup_error.code()))?,
                }
            } else {
                setup
                    .complete_startup(false, Some("setup_recovery_failed"))
                    .map_err(|error| stable_setup_error(error.code()))?;
            }
            app.manage(state);
            app.manage(setup);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_setup_state,
            get_setup_catalogue,
            prepare_setup,
            commit_setup,
            cancel_setup,
            restart_desktop,
            run_agent_turn
        ])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}

fn stable_setup_error(code: &'static str) -> std::io::Error {
    std::io::Error::other(code)
}
