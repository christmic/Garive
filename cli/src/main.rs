fn main() {
    let input = std::env::args().nth(1).unwrap_or_else(|| "hello".into());
    let host = garive_runtime::FakeHost::from_fixture(include_bytes!(
        "../../spec/fixtures/host/fake-session.json"
    ))
    .expect("committed fake-host fixture must be valid");
    let events = host.run(&input).unwrap_or_else(|error| {
        eprintln!("garive: {error}");
        std::process::exit(2);
    });
    for event in events {
        match event.kind {
            garive_runtime::HostEventKind::OutputDelta => {
                print!("{}", event.text.as_deref().unwrap_or_default());
            }
            garive_runtime::HostEventKind::TurnCompleted => println!(),
            _ => {}
        }
    }
}
