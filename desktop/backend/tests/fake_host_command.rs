#[test]
fn command_runs_the_shared_fake_host() {
    assert_eq!(
        garive_desktop::run_fake_host("hello"),
        Ok("hello from Garive".into())
    );
}

#[test]
fn command_rejects_input_outside_the_fixture() {
    assert_eq!(
        garive_desktop::run_fake_host("different"),
        Err("fake host accepts only fixture input".into())
    );
}
