use garive_proto::com::garive::process::v1::{
    process_exit_v1, ProcessExitV1, ProcessIdentityV1, ProcessServiceStateV1,
    ProcessTerminalReceiptV1,
};
use garive_proto::{ProcessStateError, ProcessStateReducer};

#[test]
fn exact_lifecycle_retains_and_acknowledges_terminal_evidence() {
    let identity = identity();
    let mut reducer = ProcessStateReducer::new(identity.clone()).unwrap();
    reducer.start(&identity).unwrap();
    reducer.mark_running(&identity).unwrap();
    reducer.retain_terminal(receipt(identity.clone())).unwrap();
    let status = reducer.query(&identity).unwrap();
    assert_eq!(
        status.state(),
        ProcessServiceStateV1::ProcessServiceStateTerminalRetained
    );
    let digest = status.terminal.unwrap().receipt_digest;
    reducer.acknowledge(&identity, &digest).unwrap();
    assert_eq!(
        reducer.query(&identity).unwrap().state(),
        ProcessServiceStateV1::ProcessServiceStateAbsent
    );
}

#[test]
fn replay_mismatch_premature_terminal_and_wrong_ack_fail_closed() {
    let identity = identity();
    let mut other = identity.clone();
    other.dispatch_attempt_id = "other-attempt".into();
    let mut reducer = ProcessStateReducer::new(identity.clone()).unwrap();
    assert_eq!(
        reducer.query(&other),
        Err(ProcessStateError::IdentityMismatch)
    );
    assert_eq!(
        reducer.retain_terminal(receipt(identity.clone())),
        Err(ProcessStateError::StateConflict)
    );
    reducer.start(&identity).unwrap();
    assert_eq!(
        reducer.start(&identity),
        Err(ProcessStateError::StateConflict)
    );
    reducer.mark_running(&identity).unwrap();
    let mut invalid = receipt(identity.clone());
    invalid.process_tree_terminated = false;
    assert_eq!(
        reducer.retain_terminal(invalid),
        Err(ProcessStateError::InvalidTerminal)
    );
    reducer.retain_terminal(receipt(identity.clone())).unwrap();
    assert_eq!(
        reducer.acknowledge(&identity, &[9; 32]),
        Err(ProcessStateError::IdentityMismatch)
    );
    assert_eq!(
        reducer.terminate(&other),
        Err(ProcessStateError::IdentityMismatch)
    );
}

#[test]
fn terminated_start_authority_cannot_be_replayed() {
    let identity = identity();
    let mut reducer = ProcessStateReducer::new(identity.clone()).unwrap();
    reducer.start(&identity).unwrap();
    reducer.terminate(&identity).unwrap();
    reducer.terminate(&identity).unwrap();
    assert_eq!(
        reducer.start(&identity),
        Err(ProcessStateError::StateConflict)
    );
}

fn identity() -> ProcessIdentityV1 {
    ProcessIdentityV1 {
        protocol_revision: "guest-v1.0".into(),
        invocation_id: "inv-1".into(),
        dispatch_attempt_id: "attempt-1".into(),
        executor_revision: "exec-1".into(),
        prepared_digest: vec![1; 32],
        vm_configuration_digest: vec![2; 32],
        workload_digest: vec![3; 32],
    }
}

fn receipt(identity: ProcessIdentityV1) -> ProcessTerminalReceiptV1 {
    ProcessTerminalReceiptV1 {
        identity: Some(identity),
        exit: Some(ProcessExitV1 {
            classification: Some(process_exit_v1::Classification::Code(0)),
        }),
        stdout: b"ok\n".to_vec(),
        stderr: Vec::new(),
        truncated: false,
        process_tree_terminated: true,
        receipt_digest: Vec::new(),
    }
}
