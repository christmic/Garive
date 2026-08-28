//! Desktop command logic shared by the Tauri entry point and integration tests.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Runs the committed fake Host scenario and returns its ordered text output.
///
/// The command rejects input that does not match the fixture's admitted
/// command. It is a shell boundary until the durable Runtime host is composed.
pub fn run_fake_host(input: &str) -> Result<String, String> {
    let host = garive_runtime::FakeHost::from_fixture(include_bytes!(
        "../../../spec/fixtures/host/fake-session.json"
    ))
    .map_err(str::to_owned)?;
    let mut output = String::new();
    for event in host.run(input).map_err(str::to_owned)? {
        if event.kind == garive_runtime::HostEventKind::OutputDelta {
            output.push_str(event.text.as_deref().unwrap_or_default());
        }
    }
    Ok(output)
}
