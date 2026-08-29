#[cfg(unix)]
mod unix {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    use serde_json::{json, Value};
    use tempfile::tempdir;

    #[test]
    fn explicit_cli_runs_the_only_pipeline_and_writes_tracking() {
        let directory = tempdir().unwrap();
        let broker = directory.path().join("broker.sh");
        let agent = directory.path().join("agent.sh");
        let evaluator = directory.path().join("evaluator.sh");
        executable(&broker, BROKER);
        executable(&agent, AGENT);
        executable(&evaluator, EVALUATOR);
        let predictions = directory.path().join("predictions");
        fs::create_dir(&predictions).unwrap();
        let tracking = directory.path().join("run.jsonl");
        let config_path = directory.path().join("config.json");
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let cases = repository.join("cases/swe-bench-lite-smoke.jsonl");
        let config = json!({
            "cases_path":cases,"tracking_path":tracking,"dataset":"swe-bench-lite",
            "mode":"development","jobs":1,"warm_capacity":1,
            "case_limits":{"max_cases":2,"max_document_bytes":1000000,"max_line_bytes":1000000,
                "max_problem_bytes":100000,"max_tests_per_group":1000},
            "max_case_duration_ms":1000,"max_patch_bytes":10000,
            "environment_port":port(&broker),"agent_port":port(&agent),
            "official":{"python_executable":evaluator,"predictions_directory":predictions,
                "run_directory":directory.path(),"run_id":"smoke-1","model_name":"garive-1",
                "timeout_seconds":60,"environment":[]},
            "tracking":{"suite_id":"swe-bench-lite","dataset_revision":"SWE-bench/SWE-bench_Lite:test@fixture",
                "harness_revision":"7a21e05772954cc81471ae19d56f436cecf43c54","agent_revision":"test-revision",
                "dirty":true,"intake_adapter":"exact-swe-v1","patch_adapter":"unified-diff-v1"}
        });
        fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_bench"))
            .args(["run", config_path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(summary["passed"], 1);
        assert_eq!(summary["infrastructure_failed"], 0);
        let events = fs::read_to_string(tracking)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["kind"], "run-start");
        assert_eq!(events[1]["case_id"], "astropy__astropy-12907");
        assert_eq!(events[2]["score_numerator"], 1);
    }

    fn port(executable: &std::path::Path) -> Value {
        json!({"executable":executable,"arguments":[],"working_directory":"/tmp",
            "environment":[],"timeout_ms":5000,"max_output_bytes":100000})
    }

    fn executable(path: &std::path::Path, content: &str) {
        fs::write(path, content).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).unwrap();
    }

    const BROKER: &str = r#"#!/bin/sh
input=$(cat)
case "$1" in
  acquire)
    id=$(printf '%s' "$input" | sed -n 's/.*"instance_id":"\([^"]*\)".*/\1/p')
    base=$(printf '%s' "$input" | sed -n 's/.*"base_commit":"\([^"]*\)".*/\1/p')
    printf '{"handle":"workspace","case_id":"%s","base_commit":"%s"}\n' "$id" "$base" ;;
  release) printf '{"released":true}\n' ;;
  *) exit 9 ;;
esac
"#;

    const AGENT: &str = r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"raw":"diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n","duration_ms":10,"input_tokens":3,"output_tokens":2}'
"#;

    const EVALUATOR: &str = r#"#!/bin/sh
run_id=
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--run_id" ]; then shift; run_id=$1; fi
  shift
done
cat > "garive-1.${run_id}.json" <<'EOF'
{"total_instances":1,"submitted_instances":1,"completed_instances":1,"resolved_instances":1,"unresolved_instances":0,"empty_patch_instances":0,"error_instances":0,"completed_ids":["astropy__astropy-12907"],"incomplete_ids":[],"empty_patch_ids":[],"submitted_ids":["astropy__astropy-12907"],"resolved_ids":["astropy__astropy-12907"],"unresolved_ids":[],"error_ids":[],"schema_version":2}
EOF
"#;
}
