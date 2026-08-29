use std::{fs, path::Path};

use bench::{parse_explicit_config, BenchErrorCode};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    value: u64,
}

#[test]
fn explicit_configuration_rejects_duplicates_and_has_canonical_digest() {
    let (first, first_digest): (Config, String) = parse_explicit_config(br#"{"value":1}"#).unwrap();
    let (_, second_digest): (Config, String) = parse_explicit_config(b"{ \"value\" : 1 }").unwrap();
    assert_eq!(first.value, 1);
    assert_eq!(first_digest, second_digest);
    assert_eq!(
        parse_explicit_config::<Config>(br#"{"value":1,"value":2}"#)
            .unwrap_err()
            .code(),
        BenchErrorCode::InvalidConfiguration
    );
}

#[test]
fn evaluation_engine_remains_pure_and_benchmark_reads_no_environment_configuration() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let eval = sources(&root.join("engine/eval/src"));
    for forbidden in ["std::fs", "std::net", "std::process", "reqwest", "tokio::"] {
        assert!(
            !eval.contains(forbidden),
            "Engine eval owns forbidden I/O: {forbidden}"
        );
    }
    let bench = sources(&root.join("bench/src"));
    for forbidden in ["std::env::var", "std::env::vars", "var_os("] {
        assert!(
            !bench.contains(forbidden),
            "B0 discovered configuration from environment: {forbidden}"
        );
    }
    assert!(bench.contains(".env_clear()"));
}

fn sources(root: &Path) -> String {
    let mut paths = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| fs::read_to_string(path).unwrap())
        .collect()
}
