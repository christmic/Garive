#[tauri::command]
fn run_fake_host(input: String) -> Result<String, String> {
    let host = garive_runtime::FakeHost::from_fixture(include_bytes!(
        "../../../spec/fixtures/host/fake-session.json"
    )).map_err(str::to_owned)?;
    let mut output = String::new();
    for event in host.run(&input).map_err(str::to_owned)? {
        if event.kind == garive_runtime::HostEventKind::OutputDelta {
            output.push_str(event.text.as_deref().unwrap_or_default());
        }
    }
    Ok(output)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_fake_host])
        .run(tauri::generate_context!())
        .expect("Garive desktop runtime failed");
}
