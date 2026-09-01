use garive_proto::com::garive::process::v1::{
    process_exit_v1, ProcessEnvironmentEntryV1, ProcessExitV1, ProcessIdentityV1,
    ProcessTerminalReceiptV1, ProcessWorkloadV1, ProcessWorkspaceModeV1,
};
use garive_proto::{
    process_receipt_digest, process_workload_digest, ProcessDigestError,
    PROCESS_PROTOCOL_REVISION_V1,
};

fn identity() -> ProcessIdentityV1 {
    ProcessIdentityV1 {
        protocol_revision: "guest-v1.0".into(),
        invocation_id: "inv-1".into(),
        dispatch_attempt_id: "attempt-1".into(),
        executor_revision: "exec-1".into(),
        prepared_digest: vec![1; 32],
        vm_configuration_digest: vec![2; 32],
        workload_digest: Vec::new(),
    }
}

fn workload() -> ProcessWorkloadV1 {
    ProcessWorkloadV1 {
        lane: "build".into(),
        executable: "/usr/bin/swift".into(),
        argv: vec!["swift".into(), "test".into()],
        working_directory: "project".into(),
        environment: vec![ProcessEnvironmentEntryV1 {
            key: "LANG".into(),
            value: "C.UTF-8".into(),
        }],
        max_output_bytes: 1_048_576,
        timeout_milliseconds: 300_000,
        max_processes: 64,
        max_open_files: 256,
        workspace_mode: ProcessWorkspaceModeV1::ProcessWorkspaceModeReadOnly.into(),
    }
}

#[test]
fn workload_digest_matches_the_cross_language_vector() {
    assert_eq!(
        process_workload_digest(&identity(), &workload()).unwrap(),
        bytes("570a130149d364aacd0929f6e6605a46005fc28e28b486a16fcdcbafe000c13d")
    );
}

#[test]
fn every_canonical_workload_input_is_digest_bound() {
    let base_identity = identity();
    let base_workload = workload();
    let base = process_workload_digest(&base_identity, &base_workload).unwrap();
    let mut identities = Vec::new();
    macro_rules! identity_variant {
        ($field:ident, $value:expr) => {{
            let mut value = base_identity.clone();
            value.$field = $value;
            identities.push(value);
        }};
    }
    identity_variant!(invocation_id, "inv-2".into());
    identity_variant!(dispatch_attempt_id, "attempt-2".into());
    identity_variant!(executor_revision, "exec-2".into());
    identity_variant!(prepared_digest, vec![9; 32]);
    identity_variant!(vm_configuration_digest, vec![9; 32]);
    for value in identities {
        assert_ne!(
            process_workload_digest(&value, &base_workload).unwrap(),
            base
        );
    }
    let mut wrong_protocol = base_identity.clone();
    wrong_protocol.protocol_revision = "guest-v1.1".into();
    assert_eq!(
        process_workload_digest(&wrong_protocol, &base_workload),
        Err(ProcessDigestError::InvalidIdentity)
    );
    assert_eq!(PROCESS_PROTOCOL_REVISION_V1, "guest-v1.0");
    let mut workloads = Vec::new();
    macro_rules! workload_variant {
        ($field:ident, $value:expr) => {{
            let mut value = base_workload.clone();
            value.$field = $value;
            workloads.push(value);
        }};
    }
    workload_variant!(lane, "test".into());
    workload_variant!(executable, "/usr/bin/env".into());
    workload_variant!(argv, vec!["test".into(), "swift".into()]);
    workload_variant!(working_directory, "other".into());
    workload_variant!(
        environment,
        vec![ProcessEnvironmentEntryV1 {
            key: "LANG".into(),
            value: "C".into()
        }]
    );
    workload_variant!(max_output_bytes, 1_048_575);
    workload_variant!(timeout_milliseconds, 299_999);
    workload_variant!(max_processes, 63);
    workload_variant!(max_open_files, 255);
    workload_variant!(
        workspace_mode,
        ProcessWorkspaceModeV1::ProcessWorkspaceModeReadWrite.into()
    );
    for value in workloads {
        assert_ne!(
            process_workload_digest(&base_identity, &value).unwrap(),
            base
        );
    }
}

#[test]
fn workload_digest_rejects_invalid_identity_order_bounds_and_paths() {
    let mut invalid_identity = identity();
    invalid_identity.prepared_digest.pop();
    assert_eq!(
        process_workload_digest(&invalid_identity, &workload()),
        Err(ProcessDigestError::InvalidIdentity)
    );
    let mut mismatched = identity();
    mismatched.workload_digest = vec![9; 32];
    assert_eq!(
        process_workload_digest(&mismatched, &workload()),
        Err(ProcessDigestError::InvalidIdentity)
    );

    let mut invalid = workload();
    invalid.environment = vec![
        ProcessEnvironmentEntryV1 {
            key: "Z".into(),
            value: "1".into(),
        },
        ProcessEnvironmentEntryV1 {
            key: "A".into(),
            value: "2".into(),
        },
    ];
    assert_eq!(
        process_workload_digest(&identity(), &invalid),
        Err(ProcessDigestError::InvalidWorkload)
    );
    invalid = workload();
    invalid.working_directory = "../escape".into();
    assert_eq!(
        process_workload_digest(&identity(), &invalid),
        Err(ProcessDigestError::InvalidWorkload)
    );
    invalid = workload();
    invalid.argv = vec!["x".repeat(16_385)];
    assert_eq!(
        process_workload_digest(&identity(), &invalid),
        Err(ProcessDigestError::InvalidWorkload)
    );
}

#[test]
fn receipt_digest_matches_vector_and_requires_terminal_proof() {
    let workload_digest = process_workload_digest(&identity(), &workload()).unwrap();
    let mut receipt_identity = identity();
    receipt_identity.workload_digest = workload_digest.to_vec();
    let receipt = ProcessTerminalReceiptV1 {
        identity: Some(receipt_identity),
        exit: Some(ProcessExitV1 {
            classification: Some(process_exit_v1::Classification::Code(0)),
        }),
        stdout: b"ok\n".to_vec(),
        stderr: Vec::new(),
        truncated: false,
        process_tree_terminated: true,
        receipt_digest: Vec::new(),
    };
    assert_eq!(
        process_receipt_digest(&receipt).unwrap(),
        bytes("23860e0a2c08e0f3c05157b3c85c4b8f53c3b54a9ddef81ff0bd130337abcb84")
    );

    let mut invalid = receipt.clone();
    invalid.receipt_digest = vec![9; 32];
    assert_eq!(
        process_receipt_digest(&invalid),
        Err(ProcessDigestError::InvalidReceipt)
    );
    invalid = receipt.clone();
    invalid.process_tree_terminated = false;
    assert_eq!(
        process_receipt_digest(&invalid),
        Err(ProcessDigestError::InvalidReceipt)
    );
    invalid = receipt;
    invalid.exit = Some(ProcessExitV1 {
        classification: Some(process_exit_v1::Classification::TimedOut(false)),
    });
    assert_eq!(
        process_receipt_digest(&invalid),
        Err(ProcessDigestError::InvalidReceipt)
    );
}

fn bytes(value: &str) -> [u8; 32] {
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
    }
    output
}
