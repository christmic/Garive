use garive_proto::com::garive::process::v1::{
    process_guest_request_v1, process_guest_response_v1, process_host_request_v1,
    process_host_response_v1, MacOsProcessVmPlanV1, ProcessDispatchV1, ProcessEnvironmentEntryV1,
    ProcessGuestExecuteV1, ProcessGuestRequestV1, ProcessGuestResponseV1, ProcessHostRequestV1,
    ProcessHostResponseV1, ProcessIdentityV1, ProcessProtocolErrorV1, ProcessWorkloadV1,
    ProcessWorkspaceModeV1,
};
use prost::Message;

fn identity() -> ProcessIdentityV1 {
    ProcessIdentityV1 {
        protocol_revision: "guest-v1.0".into(),
        invocation_id: "invocation-1".into(),
        dispatch_attempt_id: "attempt-1".into(),
        executor_revision: "executor-1".into(),
        prepared_digest: vec![1; 32],
        vm_configuration_digest: vec![2; 32],
        workload_digest: vec![3; 32],
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
fn generated_host_and_guest_commands_round_trip_without_shape_duplication() {
    let dispatch = ProcessDispatchV1 {
        identity: Some(identity()),
        vm_plan: Some(MacOsProcessVmPlanV1 {
            kernel_url: "file:///kernel".into(),
            kernel_digest: vec![4; 32],
            initial_ramdisk_url: "file:///initrd".into(),
            initial_ramdisk_digest: vec![5; 32],
            root_disk_url: "file:///root.raw".into(),
            root_disk_digest: vec![6; 32],
            workspace_url: "file:///workspace/".into(),
            workspace_mode: ProcessWorkspaceModeV1::ProcessWorkspaceModeReadOnly.into(),
            cpu_count: 2,
            memory_size_bytes: 1_073_741_824,
            control_timeout_milliseconds: 5_000,
        }),
        workload: Some(workload()),
    };
    let host = ProcessHostRequestV1 {
        command: Some(process_host_request_v1::Command::Start(dispatch)),
    };
    assert_eq!(
        ProcessHostRequestV1::decode(host.encode_to_vec().as_slice()).unwrap(),
        host
    );

    let guest = ProcessGuestRequestV1 {
        command: Some(process_guest_request_v1::Command::Execute(
            ProcessGuestExecuteV1 {
                identity: Some(identity()),
                workload: Some(workload()),
            },
        )),
    };
    assert_eq!(
        ProcessGuestRequestV1::decode(guest.encode_to_vec().as_slice()).unwrap(),
        guest
    );
}

#[test]
fn envelope_direction_tags_are_disjoint() {
    let host = ProcessHostRequestV1 {
        command: Some(process_host_request_v1::Command::Query(Default::default())),
    }
    .encode_to_vec();
    let host_response = ProcessHostResponseV1 {
        result: Some(process_host_response_v1::Result::Error(
            ProcessProtocolErrorV1::default(),
        )),
    }
    .encode_to_vec();
    let guest_request = ProcessGuestRequestV1 {
        command: Some(process_guest_request_v1::Command::Terminate(
            Default::default(),
        )),
    }
    .encode_to_vec();
    let guest_response = ProcessGuestResponseV1 {
        result: Some(process_guest_response_v1::Result::Error(
            ProcessProtocolErrorV1::default(),
        )),
    }
    .encode_to_vec();
    assert_eq!(first_tag(&host), 12);
    assert_eq!(first_tag(&host_response), 23);
    assert_eq!(first_tag(&guest_request), 32);
    assert_eq!(first_tag(&guest_response), 42);
}

fn first_tag(bytes: &[u8]) -> u32 {
    let mut key = 0_u64;
    for (index, byte) in bytes.iter().enumerate() {
        key |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return (key >> 3) as u32;
        }
    }
    panic!("missing protobuf field tag")
}
