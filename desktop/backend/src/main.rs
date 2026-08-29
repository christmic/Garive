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
        .manage(garive_desktop::DesktopState::default())
        .invoke_handler(tauri::generate_handler![run_agent_turn])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}
