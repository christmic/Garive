use std::{fs, path::PathBuf};

fn sources() -> Vec<(PathBuf, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    [
        "effect_batch_facts.rs",
        "effect_batch_recovery.rs",
        "effect_batch_runtime.rs",
        "effect_batch_sqlite.rs",
        "confined_read_executor.rs",
    ]
    .into_iter()
    .map(|name| {
        let path = root.join(name);
        let source = fs::read_to_string(&path).unwrap();
        (path, source)
    })
    .collect()
}

#[test]
fn batch_runtime_has_no_environment_or_speculative_path() {
    for (path, source) in sources() {
        for forbidden in ["std::env", "env::var", "speculative"] {
            assert!(
                !source.contains(forbidden),
                "{} contains forbidden {forbidden}",
                path.display()
            );
        }
    }
}

#[test]
fn batch_runtime_uses_only_explicit_bounded_buffers_and_tasks() {
    let sources = sources();
    let runtime = sources
        .iter()
        .find(|(path, _)| path.ends_with("effect_batch_runtime.rs"))
        .unwrap()
        .1
        .as_str();
    let confined = sources
        .iter()
        .find(|(path, _)| path.ends_with("confined_read_executor.rs"))
        .unwrap()
        .1
        .as_str();
    assert!(!runtime.contains("tokio::spawn("));
    assert!(runtime.contains("Semaphore::new(limits.max_parallel_reads)"));
    assert!(runtime.contains("vec![None; invocations.len()]"));
    assert!(confined.contains("take(bound.saturating_add(1))"));
    assert!(!confined.contains("read_to_string("));
}
