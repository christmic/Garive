use tauri::{Emitter, Manager};
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ProductWorkspaceTurnCommand {
    command_id: String,
    session_id: String,
    input: String,
    workspace_id: String,
    entry_ids: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ArtifactCommand {
    session_id: String,
    artifact_id: String,
    revision: u64,
    committed_position: u64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ArtifactExportCommand {
    session_id: String,
    artifact_id: String,
    revision: u64,
    committed_position: u64,
    export_target_id: String,
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
async fn resolve_turn_approval(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    turn_id: String,
    suspension_id: String,
    session_version: u64,
    approved: bool,
) -> Result<garive_desktop::DesktopTurnResult, String> {
    state
        .continue_approval_isolated(
            session_id,
            turn_id,
            suspension_id,
            session_version,
            approved,
        )
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_desktop_capabilities(
    app: tauri::AppHandle,
    state: tauri::State<'_, garive_desktop::DesktopState>,
) -> garive_desktop::DesktopCapabilityManifest {
    let mut capabilities = state.capabilities();
    capabilities.setup = true;
    capabilities.updater =
        garive_desktop::desktop_updater_configured(app.config().plugins.0.get("updater"));
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
async fn authorize_workspace_writes(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    workspace_id: String,
) -> Result<Option<garive_desktop::DesktopWorkspaceGrant>, String> {
    let Some(selection) = app
        .dialog()
        .file()
        .set_title("Allow Garive to create files in this Workspace")
        .blocking_pick_folder()
    else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|_| "workspace_unavailable".to_owned())?;
    workspaces
        .authorize_writes(&workspace_id, &path, window.label())
        .map(Some)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn revoke_workspace(
    window: tauri::WebviewWindow,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    workspace_id: String,
    expected_grant_revision: u64,
) -> Result<garive_desktop::DesktopWorkspaceRevocationReceipt, String> {
    workspaces
        .revoke(&workspace_id, expected_grant_revision, window.label())
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
fn detach_workspace_from_session(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    workspace_id: String,
    grant_revision: u64,
) -> Result<garive_desktop::DesktopWorkspaceDetachment, String> {
    state
        .detach_workspace(&session_id, &workspace_id, grant_revision)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn list_artifacts(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    after_position: u64,
    limit: usize,
) -> Result<garive_desktop::DesktopArtifactPage, String> {
    state
        .artifacts(&session_id, after_position, limit)
        .map_err(|error| error.code().to_owned())
}

fn exact_artifact(
    state: &garive_desktop::DesktopState,
    session_id: &str,
    artifact_id: &str,
    revision: u64,
    committed_position: u64,
) -> Result<garive_desktop::DesktopArtifact, String> {
    let after = committed_position
        .checked_sub(1)
        .ok_or_else(|| "artifact_not_found".to_owned())?;
    state
        .artifacts(session_id, after, 1)
        .map_err(|error| error.code().to_owned())?
        .items
        .into_iter()
        .next()
        .filter(|artifact| {
            artifact.artifact_id == artifact_id
                && artifact.revision == revision
                && artifact.committed_position == committed_position
        })
        .ok_or_else(|| "artifact_not_found".to_owned())
}

#[tauri::command]
fn get_artifact_preview(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, garive_desktop::DesktopState>,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    session_id: String,
    artifact_id: String,
    revision: u64,
    committed_position: u64,
) -> Result<garive_desktop::DesktopArtifactPreview, String> {
    let artifact = exact_artifact(
        &state,
        &session_id,
        &artifact_id,
        revision,
        committed_position,
    )?;
    if artifact.preview != "text" {
        return Err("artifact_not_found".into());
    }
    let workspace_id = artifact
        .workspace_id
        .as_deref()
        .ok_or_else(|| "artifact_preview_unavailable".to_owned())?;
    workspaces
        .preview_text_artifact(
            &artifact.artifact_id,
            artifact.revision,
            workspace_id,
            &artifact.display_name,
            &artifact.content_digest,
            window.label(),
        )
        .map_err(|_| "artifact_preview_unavailable".to_owned())
}

#[tauri::command]
async fn prepare_artifact_export(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, garive_desktop::DesktopState>,
    exports: tauri::State<'_, garive_desktop::DesktopArtifactExportService>,
    request: ArtifactCommand,
) -> Result<Option<garive_desktop::DesktopArtifactExportTarget>, String> {
    let artifact = exact_artifact(
        &state,
        &request.session_id,
        &request.artifact_id,
        request.revision,
        request.committed_position,
    )?;
    if !artifact.exportable {
        return Err("artifact_export_stale".into());
    }
    let Some(selection) = app
        .dialog()
        .file()
        .set_title("Export a new Artifact copy")
        .set_file_name(artifact.display_name)
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|_| "artifact_export_stale".to_owned())?;
    exports
        .admit_selected(&path, window.label())
        .map(Some)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn commit_artifact_export(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, garive_desktop::DesktopState>,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    exports: tauri::State<'_, garive_desktop::DesktopArtifactExportService>,
    request: ArtifactExportCommand,
) -> Result<garive_desktop::DesktopArtifactExportReceipt, String> {
    let artifact = exact_artifact(
        &state,
        &request.session_id,
        &request.artifact_id,
        request.revision,
        request.committed_position,
    )?;
    let workspace_id = artifact
        .workspace_id
        .as_deref()
        .filter(|_| artifact.exportable)
        .ok_or_else(|| "artifact_export_stale".to_owned())?;
    let bytes = workspaces
        .read_committed_artifact(
            &artifact.artifact_id,
            artifact.revision,
            workspace_id,
            &artifact.display_name,
            &artifact.content_digest,
            window.label(),
        )
        .map_err(|_| "artifact_export_stale".to_owned())?
        .into_bytes();
    exports
        .export(
            &request.export_target_id,
            window.label(),
            &artifact.artifact_id,
            artifact.revision,
            &artifact.content_digest,
            &bytes,
        )
        .map_err(|error| error.code().to_owned())
}

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

#[tauri::command]
fn set_desktop_menu_locale(app: tauri::AppHandle, locale: String) -> Result<(), String> {
    let locale = garive_desktop::DesktopMenuLocale::from_id(&locale)
        .ok_or_else(|| "invalid_locale".to_owned())?;
    let menu = garive_desktop::build_desktop_menu_for_locale(&app, locale)
        .map_err(|_| "menu_unavailable".to_owned())?;
    app.set_menu(menu)
        .map(|_| ())
        .map_err(|_| "menu_unavailable".to_owned())
}

#[tauri::command]
fn read_client_preferences(
    store: tauri::State<'_, garive_desktop::DesktopProductStore>,
) -> Result<Option<Vec<u8>>, String> {
    store
        .read_preferences()
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn write_client_preferences(
    store: tauri::State<'_, garive_desktop::DesktopProductStore>,
    value: Vec<u8>,
) -> Result<(), String> {
    store
        .write_preferences(&value)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn read_pending_command(
    store: tauri::State<'_, garive_desktop::DesktopProductStore>,
) -> Result<Option<Vec<u8>>, String> {
    store
        .read_pending()
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn write_pending_command(
    store: tauri::State<'_, garive_desktop::DesktopProductStore>,
    value: Option<Vec<u8>>,
) -> Result<(), String> {
    store
        .write_pending(value.as_deref())
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn read_pending_update(
    store: tauri::State<'_, garive_desktop::DesktopProductStore>,
) -> Result<Option<Vec<u8>>, String> {
    store
        .read_update_pending()
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn write_pending_update(
    store: tauri::State<'_, garive_desktop::DesktopProductStore>,
    value: Option<Vec<u8>>,
) -> Result<(), String> {
    store
        .write_update_pending(value.as_deref())
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_agent_definitions(
    state: tauri::State<'_, garive_desktop::DesktopState>,
) -> Result<garive_desktop::DesktopDefinitionPage, String> {
    state.definitions().map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_product_sessions(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    limit: usize,
    before: Option<String>,
) -> Result<garive_desktop::DesktopSessionPage, String> {
    state
        .sessions(limit, before.as_deref())
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_product_timeline(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    after_position: u64,
    limit: usize,
) -> Result<garive_desktop::DesktopProductTimelinePage, String> {
    state
        .product_timeline(&session_id, after_position, limit)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn create_product_session(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    command_id: String,
    definition_id: String,
) -> Result<garive_desktop::DesktopCreateSessionReceipt, String> {
    state
        .create_session_command(&command_id, &definition_id)
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
async fn start_product_turn(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    command_id: String,
    session_id: String,
    input: String,
) -> Result<garive_desktop::DesktopTurnCommandReceipt, String> {
    state
        .start_turn_detached(command_id, session_id, input)
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
async fn start_product_turn_with_workspace_context(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, garive_desktop::DesktopState>,
    workspaces: tauri::State<'_, garive_desktop::DesktopWorkspaceService>,
    request: ProductWorkspaceTurnCommand,
) -> Result<garive_desktop::DesktopTurnCommandReceipt, String> {
    let context = workspaces
        .read_context_files(&request.workspace_id, window.label(), &request.entry_ids)
        .map_err(|error| error.code().to_owned())?;
    state
        .start_turn_with_context_detached(
            request.command_id,
            request.session_id,
            request.input,
            context,
        )
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn cancel_product_turn(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    command_id: String,
    session_id: String,
    turn_id: String,
    requested_through_position: u64,
) -> Result<garive_desktop::DesktopTurnCommandReceipt, String> {
    state
        .cancel_turn_command(
            &command_id,
            &session_id,
            &turn_id,
            requested_through_position,
        )
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn continue_product_turn(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    command_id: String,
    session_id: String,
    turn_id: String,
    suspension_id: String,
    session_version: u64,
    input: String,
) -> Result<garive_desktop::DesktopTurnCommandReceipt, String> {
    state
        .continue_turn_detached(
            command_id,
            session_id,
            turn_id,
            suspension_id,
            session_version,
            input,
        )
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn continue_product_approval(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    command_id: String,
    session_id: String,
    turn_id: String,
    suspension_id: String,
    session_version: u64,
    approved: bool,
) -> Result<garive_desktop::DesktopTurnCommandReceipt, String> {
    state
        .continue_approval_detached(
            command_id,
            session_id,
            turn_id,
            suspension_id,
            session_version,
            approved,
        )
        .await
        .map_err(|error| error.code().to_owned())
}

#[tauri::command]
fn get_session_events(
    state: tauri::State<'_, garive_desktop::DesktopState>,
    session_id: String,
    after_position: u64,
) -> Result<garive_desktop::DesktopEventPage, String> {
    state
        .event_page(&session_id, after_position)
        .map_err(|error| error.code().to_owned())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(garive_desktop::build_desktop_menu)
        .on_menu_event(|app, event| {
            if let Some(intent) = garive_desktop::DesktopMenuIntent::from_id(event.id().as_ref()) {
                let _ = app.emit_to("main", garive_desktop::DESKTOP_MENU_EVENT, intent.id());
            }
        })
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
            let product_store = garive_desktop::DesktopProductStore::new(
                app.path()
                    .app_data_dir()
                    .map_err(|_| stable_setup_error("data_directory"))?
                    .join("product"),
            )
            .map_err(|error| stable_setup_error(error.code()))?;
            let workspaces = garive_desktop::DesktopWorkspaceService::durable(
                directory.join(garive_desktop::DESKTOP_WORKSPACE_MANIFEST_FILE),
                std::sync::Arc::new(garive_desktop::SystemDesktopWorkspaceBookmarkStore),
            );
            let state = garive_desktop::DesktopState::default();
            let recovered = setup.recover(false).is_ok();
            if recovered {
                match state.install_from_with_workspaces(&provider, workspaces.clone(), "main") {
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
            // Workspace authorization can become unavailable independently of
            // the Runtime configuration; never make a stale bookmark prevent
            // the Desktop shell from launching.
            let _workspace_recovery = workspaces.recover("main");
            app.manage(state);
            app.manage(setup);
            app.manage(workspaces);
            app.manage(product_store);
            app.manage(
                garive_desktop::DesktopArtifactExportService::durable(
                    directory.join(garive_desktop::DESKTOP_ARTIFACT_EXPORT_JOURNAL_FILE),
                )
                .map_err(|error| stable_setup_error(error.code()))?,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_desktop_menu_locale,
            get_desktop_capabilities,
            get_setup_state,
            get_setup_catalogue,
            prepare_setup,
            commit_setup,
            cancel_setup,
            choose_workspace,
            verify_workspace,
            get_workspace_recovery_status,
            list_workspace_authorizations,
            reauthorize_workspace,
            authorize_workspace_writes,
            revoke_workspace,
            list_workspace_entries,
            create_work_session,
            attach_workspace_to_session,
            get_session_workspaces,
            detach_workspace_from_session,
            list_artifacts,
            get_artifact_preview,
            prepare_artifact_export,
            commit_artifact_export,
            restart_desktop,
            get_recent_sessions,
            get_session_timeline,
            continue_agent_turn,
            resolve_turn_approval,
            run_agent_turn_with_workspace_context,
            run_agent_turn,
            read_client_preferences,
            write_client_preferences,
            read_pending_command,
            write_pending_command,
            read_pending_update,
            write_pending_update,
            get_agent_definitions,
            get_product_sessions,
            get_product_timeline,
            create_product_session,
            start_product_turn,
            start_product_turn_with_workspace_context,
            cancel_product_turn,
            continue_product_turn,
            continue_product_approval,
            get_session_events
        ])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}

fn stable_setup_error(code: &'static str) -> std::io::Error {
    std::io::Error::other(code)
}
