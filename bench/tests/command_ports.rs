use bench::{
    AgentDriver, AgentInput, BenchErrorCode, CommandAgentDriver, CommandEnvironmentPool,
    CommandPortConfig, EnvironmentPool, WorkspaceLease,
};
use garive_eval::EvaluationCaseId;

#[tokio::test]
async fn command_broker_and_agent_use_exact_json_with_cleared_environment() {
    let broker = CommandEnvironmentPool::new(config(BROKER, 1_024), 2).unwrap();
    let case = case();
    let lease = broker.acquire(&case).await.unwrap();
    assert_eq!(lease.handle, "workspace");
    assert_eq!(lease.case_id, "case-0");
    broker.release(lease.clone()).await.unwrap();

    let agent = CommandAgentDriver::new(config(AGENT, 2_048)).unwrap();
    let output = agent
        .run(
            AgentInput {
                payload: "problem".into(),
                repository: "owner/repo".into(),
                base_commit: "a".repeat(40),
                workspace_handle: "workspace".into(),
            },
            &lease,
        )
        .await
        .unwrap();
    assert!(output.raw.starts_with("diff --git "));
    assert_eq!(output.duration_ms, 12);
    assert_eq!(output.input_tokens, Some(4));
    assert_eq!(output.output_tokens, Some(3));
}

#[tokio::test]
async fn timeout_size_exit_and_unknown_json_fail_as_infrastructure() {
    for (script, timeout, maximum) in [
        ("sleep 1; echo '{\"raw\":\"x\",\"duration_ms\":1,\"input_tokens\":null,\"output_tokens\":null}'", 10, 1_024),
        ("printf '%0200d' 0", 1_000, 16),
        ("exit 9", 1_000, 1_024),
        ("echo '{\"raw\":\"x\",\"duration_ms\":1,\"input_tokens\":null,\"output_tokens\":null,\"future\":true}'", 1_000, 1_024),
    ] {
        let mut value = config(script, maximum);
        value.timeout_ms = timeout;
        let agent = CommandAgentDriver::new(value).unwrap();
        let result = agent.run(input(), &lease()).await;
        assert_eq!(result.unwrap_err().code(), BenchErrorCode::InfrastructureFailure);
    }
}

#[test]
fn command_configuration_is_explicit_and_bounded() {
    let mut duplicate = config(AGENT, 100);
    duplicate.environment = vec![("A".into(), "1".into()), ("A".into(), "2".into())];
    assert!(CommandAgentDriver::new(duplicate).is_err());
    let mut zero = config(AGENT, 100);
    zero.timeout_ms = 0;
    assert!(CommandAgentDriver::new(zero).is_err());
    assert!(CommandEnvironmentPool::new(config(BROKER, 100), 0).is_err());
}

const BROKER: &str = r#"
read request
case "$1" in
  acquire) echo '{"handle":"workspace","case_id":"case-0","base_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}' ;;
  release) echo '{"released":true}' ;;
  *) exit 7 ;;
esac
"#;

const AGENT: &str = r#"
read request
if [ "$1" != "run" ]; then exit 7; fi
printf '%s\n' '{"raw":"diff --git a/x b/x\n","duration_ms":12,"input_tokens":4,"output_tokens":3}'
"#;

fn config(script: &str, max_output_bytes: usize) -> CommandPortConfig {
    CommandPortConfig {
        executable: "/bin/sh".into(),
        arguments: vec!["-c".into(), script.into(), "port".into()],
        working_directory: "/tmp".into(),
        environment: vec![],
        timeout_ms: 1_000,
        max_output_bytes,
    }
}

fn case() -> bench::SweCase {
    bench::SweCase {
        instance_id: EvaluationCaseId::new("case-0").unwrap(),
        repository: "owner/repo".into(),
        base_commit: "a".repeat(40),
        problem_statement: "problem".into(),
        version: "1".into(),
        fail_to_pass: vec!["test".into()],
        pass_to_pass: vec![],
    }
}
fn lease() -> WorkspaceLease {
    WorkspaceLease {
        handle: "workspace".into(),
        case_id: "case-0".into(),
        base_commit: "a".repeat(40),
    }
}
fn input() -> AgentInput {
    AgentInput {
        payload: "problem".into(),
        repository: "owner/repo".into(),
        base_commit: "a".repeat(40),
        workspace_handle: "workspace".into(),
    }
}
