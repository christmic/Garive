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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app
                .path()
                .app_config_dir()
                .map_err(|_| stable_setup_error("config_directory"))?;
            let provider = garive_desktop::FileDesktopConfigurationProvider::new(
                directory.join(garive_desktop::DESKTOP_CONFIG_FILE),
                directory,
                garive_desktop::SystemDesktopSecretResolver,
                garive_desktop::BuiltinDesktopProfileRegistry,
            );
            let state = garive_desktop::DesktopState::default();
            state
                .install_from(&provider)
                .map_err(|error| stable_setup_error(error.code()))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![run_agent_turn])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}

fn stable_setup_error(code: &'static str) -> std::io::Error {
    std::io::Error::other(code)
}
