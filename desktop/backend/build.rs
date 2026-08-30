fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_setup_state",
            "get_setup_catalogue",
            "prepare_setup",
            "commit_setup",
            "cancel_setup",
            "restart_desktop",
            "run_agent_turn",
        ]),
    ))
    .expect("failed to build Garive Desktop permissions")
}
