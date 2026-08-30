use tauri::Manager;

#[tauri::command]
async fn run_agent_turn(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    definition_id: String,
    session_id: Option<String>,
    input: String,
) -> Result<garive_desktop::DesktopTurnResult, String> {
    state
        .run_turn_in_session_isolated(definition_id, session_id, input)
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_desktop_capabilities(
    state: tauri::State<'_, garive_desktop::DesktopState>,
) -> garive_desktop::DesktopCapabilityManifest {
    state.capabilities()
}

#[tauri::command]
fn get_recent_sessions(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    limit: usize,
) -> Result<Vec<garive_desktop::DesktopSessionSummary>, String> {
    state
        .recent_sessions(limit)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_session_timeline(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    after_position: u64,
    limit: usize,
) -> Result<garive_desktop::DesktopTimelinePage, String> {
    state
        .session_timeline(&session_id, after_position, limit)
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
        .invoke_handler(tauri::generate_handler![
            get_desktop_capabilities,
            get_recent_sessions,
            get_session_timeline,
            run_agent_turn
        ])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}

fn stable_setup_error(code: &'static str) -> std::io::Error {
    std::io::Error::other(code)
}
