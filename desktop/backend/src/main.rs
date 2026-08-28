#[tauri::command]
fn run_fake_host(input: String) -> Result<String, String> {
    garive_desktop::run_fake_host(&input)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_fake_host])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}
