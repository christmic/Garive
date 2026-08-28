fn main() {
    let host = garive_runtime::FakeHost::from_fixture(include_bytes!(
        "../../spec/fixtures/host/fake-session.json"
    ))
    .expect("committed fake-host fixture must be valid");
    println!("┌─ Garive Agent ─────────────────────────┐");
    println!("│ You: hello");
    print!("│ Agent: ");
    for event in host.run("hello").expect("fixture command must run") {
        if event.kind == garive_runtime::HostEventKind::OutputDelta {
            print!("{}", event.text.as_deref().unwrap_or_default());
        }
    }
    println!("\n└─ completed @ position 5 ──────────────┘");
}
