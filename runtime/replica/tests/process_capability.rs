use garive_runtime::{ProcessExecutable, ProcessLane, ProcessLaneRegistry};

fn executable(alias: &str, path: &str) -> ProcessExecutable {
    ProcessExecutable::new(alias, path).expect("valid executable")
}

#[test]
fn registry_resolves_only_exact_configured_lanes_and_aliases() {
    let lane = ProcessLane::new(
        "rust-toolchain",
        [
            executable("cargo", "/opt/garive/bin/cargo"),
            executable("rustc", "/opt/garive/bin/rustc"),
        ],
        [("CARGO_HOME".into(), "/opt/garive/cargo-home".into())],
    )
    .expect("lane");
    let registry = ProcessLaneRegistry::new([lane]).expect("registry");

    let resolved = registry.lane("rust-toolchain").expect("exact lane");
    assert_eq!(
        resolved.executable("cargo").expect("exact alias").path(),
        std::path::Path::new("/opt/garive/bin/cargo")
    );
    assert_eq!(
        resolved.environment().get("CARGO_HOME").map(String::as_str),
        Some("/opt/garive/cargo-home")
    );
    assert!(resolved.executable("Cargo").is_none());
    assert!(registry.lane("RUST-TOOLCHAIN").is_none());
    assert_eq!(
        registry.lane_names().collect::<Vec<_>>(),
        ["rust-toolchain"]
    );
}

#[test]
fn lane_debug_exposes_environment_keys_but_never_values() {
    let lane = ProcessLane::new(
        "rust",
        [ProcessExecutable::new("cargo", "/opt/garive/bin/cargo").unwrap()],
        [("ACCESS_TOKEN".into(), "secret-never-log".into())],
    )
    .unwrap();
    let debug = format!("{lane:?}");
    assert!(debug.contains("ACCESS_TOKEN"));
    assert!(!debug.contains("secret-never-log"));
}

#[test]
fn executable_capability_requires_alias_and_absolute_path() {
    for (alias, path) in [
        ("", "/bin/true"),
        ("bin/true", "/bin/true"),
        ("true", "bin/true"),
        ("bad\0alias", "/bin/true"),
    ] {
        assert!(ProcessExecutable::new(alias, path).is_err());
    }
}

#[test]
fn lane_rejects_implicit_or_ambiguous_configuration() {
    assert!(ProcessLane::new(
        "rust lane",
        [executable("cargo", "/bin/cargo")],
        std::iter::empty()
    )
    .is_err());
    assert!(ProcessLane::new("rust", std::iter::empty(), std::iter::empty()).is_err());
    assert!(ProcessLane::new(
        "rust",
        [
            executable("cargo", "/bin/cargo"),
            executable("cargo", "/other/cargo"),
        ],
        std::iter::empty()
    )
    .is_err());
    assert!(ProcessLane::new(
        "rust",
        [executable("cargo", "/bin/cargo")],
        [
            ("PATH".into(), "/bin".into()),
            ("PATH".into(), "/usr/bin".into())
        ]
    )
    .is_err());
    for key in ["", "1BAD", "BAD=KEY", "BAD-KEY"] {
        assert!(ProcessLane::new(
            "rust",
            [executable("cargo", "/bin/cargo")],
            [(key.into(), "value".into())]
        )
        .is_err());
    }
}

#[test]
fn registry_rejects_empty_and_duplicate_lanes() {
    assert!(ProcessLaneRegistry::new(std::iter::empty()).is_err());
    let lane = ProcessLane::new(
        "rust",
        [executable("cargo", "/bin/cargo")],
        std::iter::empty(),
    )
    .expect("lane");
    assert!(ProcessLaneRegistry::new([lane.clone(), lane]).is_err());
}
