use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WorkspaceTurnCommand {
    definition_id: String,
    session_id: String,
    input: String,
    workspace_id: String,
    entry_ids: Vec<String>,
}

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
async fn run_agent_turn_with_workspace_context(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, garive_desktop::DesktopState>,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    request: WorkspaceTurnCommand,
) -> Result<garive_desktop::DesktopTurnResult, String> {
    let context = workspaces
        .read_context_files(&request.workspace_id, window.label(), &request.entry_ids)
        .map_err(|error| error.code().to_owned())?;
    state
        .run_turn_with_context_isolated(
            request.definition_id,
            request.session_id,
            request.input,
            context,
        )
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
async fn continue_agent_turn(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    turn_id: String,
    suspension_id: String,
    session_version: u64,
    input: String,
) -> Result<garive_desktop::DesktopTurnResult, String> {
    state
        .continue_turn_isolated(session_id, turn_id, suspension_id, session_version, input)
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_desktop_capabilities(
    state: tauri::State<'_, garive_desktop::DesktopState>,
) -> garive_desktop::DesktopCapabilityManifest {
    let mut capabilities = state.capabilities();
    capabilities.setup = true;
    capabilities
}

type SetupState = garive_desktop::DesktopSetupService<garive_desktop::SystemSetupCredentialStore>;

#[tauri::command]
async fn choose_workspace(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
) -> Result<Option<garive_desktop::DesktopWorkspaceGrant>, String> {
    let Some(selection) = app
        .dialog()
        .file()
        .set_title("Choose a Workspace for Garive")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|_| "workspace_unavailable".to_owned())?;
    workspaces
        .admit_selected(&path, window.label())
        .map(Some)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn verify_workspace(
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    workspace_id: String,
) -> Result<garive_desktop::DesktopWorkspaceGrant, String> {
    workspaces
        .verify(&workspace_id, window.label())
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_workspace_recovery_status(
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
) -> Result<garive_desktop::DesktopWorkspaceRecoveryStatus, String> {
    workspaces
        .recovery_status()
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn list_workspace_authorizations(
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
) -> Result<Vec<garive_desktop::DesktopWorkspaceAuthorization>, String> {
    workspaces
        .authorizations()
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
async fn reauthorize_workspace(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    workspace_id: String,
) -> Result<Option<garive_desktop::DesktopWorkspaceGrant>, String> {
    let Some(selection) = app
        .dialog()
        .file()
        .set_title("Restore access to this Garive Workspace")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|_| "workspace_unavailable".to_owned())?;
    workspaces
        .reauthorize(&workspace_id, &path, window.label())
        .map(Some)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn revoke_workspace(
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    workspace_id: String,
) -> Result<(), String> {
    workspaces
        .revoke(&workspace_id, window.label())
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn list_workspace_entries(
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    workspace_id: String,
    parent_entry_id: Option<String>,
    cursor: Option<String>,
    limit: usize,
) -> Result<garive_desktop::DesktopWorkspaceEntryPage, String> {
    workspaces
        .list_entries(
            &workspace_id,
            window.label(),
            parent_entry_id.as_deref(),
            cursor.as_deref(),
            limit,
        )
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn create_work_session(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    definition_id: String,
) -> Result<String, String> {
    state
        .create_session(&definition_id)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn attach_workspace_to_session(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, garive_desktop::DesktopState>,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    session_id: String,
    workspace_id: String,
) -> Result<garive_desktop::DesktopWorkspaceAttachment, String> {
    let workspace = workspaces
        .verify(&workspace_id, window.label())
        .map_err(|error| error.code().to_owned())?;
    state
        .attach_workspace(&session_id, &workspace)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_session_workspaces(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
) -> Result<Vec<garive_desktop::DesktopWorkspaceAttachment>, String> {
    state
        .session_workspaces(&session_id)
        .map_err(|error| error.code().to_owned())
}

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
fn cancel_setup(
    setup: tauri::State<'_, SetupState>,
    plan_digest: String,
) -> Result<garive_desktop::DesktopSetupCancellation, String> {
    setup
        .cancel(&plan_digest)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn restart_desktop(app: tauri::AppHandle) {
    app.restart()
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
        .plugin(tauri_plugin_dialog::init())
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
                directory.clone(),
                garive_desktop::SystemSetupCredentialStore,
            );
            let workspaces = garive_desktop::DesktopWorkspaceService::durable(
                directory.join(garive_desktop::DESKTOP_WORKSPACE_MANIFEST_FILE),
                std::sync::Arc::new(garive_desktop::SystemDesktopWorkspaceBookmarkStore),
            );
            let state = garive_desktop::DesktopState::default();
            let installed = state
                .install_from(&provider)
                .map_err(|error| stable_setup_error(error.code()))?;
            setup
                .recover(installed)
                .map_err(|error| stable_setup_error(error.code()))?;
            // Workspace authorization can become unavailable independently of
            // the Runtime configuration; never make a stale bookmark prevent
            // the Desktop shell from launching.
            let _workspace_recovery = workspaces.recover("main");
            app.manage(state);
            app.manage(setup);
            app.manage(workspaces);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_desktop_capabilities,
            get_setup_catalogue,
            prepare_setup,
            commit_setup,
            cancel_setup,
            choose_workspace,
            verify_workspace,
            get_workspace_recovery_status,
            list_workspace_authorizations,
            reauthorize_workspace,
            revoke_workspace,
            list_workspace_entries,
            create_work_session,
            attach_workspace_to_session,
            get_session_workspaces,
            restart_desktop,
            get_recent_sessions,
            get_session_timeline,
            continue_agent_turn,
            run_agent_turn_with_workspace_context,
            run_agent_turn
        ])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}

fn stable_setup_error(code: &'static str) -> std::io::Error {
    std::io::Error::other(code)
}
