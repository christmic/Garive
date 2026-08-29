use std::{fs, path::PathBuf};

use garive_tools::{recover_effect, RecoveryDecision, RecoveryPosition, ReplayClass};
use serde_json::Value;

#[test]
fn shared_recovery_matrix_never_infers_safe_replay() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/governed-effects.json");
    let fixture: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    for case in fixture["recovery_cases"].as_array().unwrap() {
        let position = match case["position"].as_str().unwrap() {
            "authorized" => RecoveryPosition::Authorized,
            "started_no_receipt" => RecoveryPosition::StartedNoReceipt,
            "receipt_no_result" => RecoveryPosition::ReceiptNoResult,
            "terminal" => RecoveryPosition::Terminal,
            other => panic!("unknown position: {other}"),
        };
        let replay = match case["replay_class"].as_str().unwrap() {
            "read_only" => ReplayClass::ReadOnly,
            "idempotent" => ReplayClass::Idempotent,
            "receipt_recoverable" => ReplayClass::ReceiptRecoverable,
            "never_replay" => ReplayClass::NeverReplay,
            other => panic!("unknown replay class: {other}"),
        };
        let expected = match case["expected"].as_str().unwrap() {
            "revalidate_grant" => RecoveryDecision::RevalidateGrant,
            "retry_same_invocation" => RecoveryDecision::RetrySameInvocation,
            "recover_executor_receipt" => RecoveryDecision::RecoverExecutorReceipt,
            "reconstruct_from_receipt" => RecoveryDecision::ReconstructFromReceipt,
            "return_terminal" => RecoveryDecision::ReturnTerminal,
            "reconcile_operator" => RecoveryDecision::ReconcileOperator,
            other => panic!("unknown decision: {other}"),
        };
        assert_eq!(
            recover_effect(
                position,
                replay,
                case["executor_proves_replay"].as_bool().unwrap()
            ),
            expected,
            "{}",
            case["name"]
        );
    }
}
