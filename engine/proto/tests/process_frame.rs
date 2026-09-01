use garive_proto::com::garive::process::v1::{
    process_guest_request_v1, process_guest_response_v1, process_host_request_v1,
    process_host_response_v1, ProcessGuestRequestV1, ProcessGuestResponseV1, ProcessHostRequestV1,
    ProcessHostResponseV1, ProcessIdentityRequestV1, ProcessProtocolErrorV1,
    ProcessProtocolFailureV1,
};
use garive_proto::{
    decode_guest_request_frame, decode_guest_response_frame, decode_host_request_frame,
    decode_host_response_frame, encode_process_frame, ProcessFrameError,
    PROCESS_FRAME_MAX_PAYLOAD_BYTES,
};

fn envelopes() -> (
    ProcessHostRequestV1,
    ProcessHostResponseV1,
    ProcessGuestRequestV1,
    ProcessGuestResponseV1,
) {
    (
        ProcessHostRequestV1 {
            command: Some(process_host_request_v1::Command::Query(
                ProcessIdentityRequestV1 {
                    identity: Some(Default::default()),
                },
            )),
        },
        ProcessHostResponseV1 {
            result: Some(process_host_response_v1::Result::Error(
                ProcessProtocolErrorV1 {
                    failure: ProcessProtocolFailureV1::ProcessProtocolFailureMalformed.into(),
                },
            )),
        },
        ProcessGuestRequestV1 {
            command: Some(process_guest_request_v1::Command::Terminate(
                ProcessIdentityRequestV1 {
                    identity: Some(Default::default()),
                },
            )),
        },
        ProcessGuestResponseV1 {
            result: Some(process_guest_response_v1::Result::Error(
                ProcessProtocolErrorV1 {
                    failure: ProcessProtocolFailureV1::ProcessProtocolFailureMalformed.into(),
                },
            )),
        },
    )
}

#[test]
fn all_four_directions_round_trip_exact_frames() {
    let (host_request, host_response, guest_request, guest_response) = envelopes();
    assert_eq!(
        decode_host_request_frame(&encode_process_frame(&host_request).unwrap()).unwrap(),
        host_request
    );
    assert_eq!(
        decode_host_response_frame(&encode_process_frame(&host_response).unwrap()).unwrap(),
        host_response
    );
    assert_eq!(
        decode_guest_request_frame(&encode_process_frame(&guest_request).unwrap()).unwrap(),
        guest_request
    );
    assert_eq!(
        decode_guest_response_frame(&encode_process_frame(&guest_response).unwrap()).unwrap(),
        guest_response
    );
}

#[test]
fn malformed_lengths_payloads_unknowns_and_duplicates_fail_closed() {
    assert_eq!(
        decode_host_request_frame(&[]),
        Err(ProcessFrameError::Malformed)
    );
    assert_eq!(
        decode_host_request_frame(&[0, 0, 0, 1]),
        Err(ProcessFrameError::Malformed)
    );
    let oversized = u32::try_from(PROCESS_FRAME_MAX_PAYLOAD_BYTES + 1)
        .unwrap()
        .to_be_bytes();
    assert_eq!(
        decode_host_request_frame(&oversized),
        Err(ProcessFrameError::BoundsExceeded)
    );

    let (host, _, _, _) = envelopes();
    let frame = encode_process_frame(&host).unwrap();
    let payload = &frame[4..];
    let mut unknown = payload.to_vec();
    unknown.extend([0x98, 0x06, 0x01]);
    assert_eq!(
        decode_host_request_frame(&framed(&unknown)),
        Err(ProcessFrameError::Malformed)
    );
    let duplicate = [payload, payload].concat();
    assert_eq!(
        decode_host_request_frame(&framed(&duplicate)),
        Err(ProcessFrameError::Malformed)
    );
}

#[test]
fn absent_body_and_wrong_direction_fail_closed() {
    let empty_host = encode_process_frame(&ProcessHostRequestV1::default()).unwrap();
    assert_eq!(
        decode_host_request_frame(&empty_host),
        Err(ProcessFrameError::Malformed)
    );

    let (host, host_response, guest, guest_response) = envelopes();
    assert_eq!(
        decode_guest_request_frame(&encode_process_frame(&host).unwrap()),
        Err(ProcessFrameError::Malformed)
    );
    assert_eq!(
        decode_guest_response_frame(&encode_process_frame(&host_response).unwrap()),
        Err(ProcessFrameError::Malformed)
    );
    assert_eq!(
        decode_host_request_frame(&encode_process_frame(&guest).unwrap()),
        Err(ProcessFrameError::Malformed)
    );
    assert_eq!(
        decode_host_response_frame(&encode_process_frame(&guest_response).unwrap()),
        Err(ProcessFrameError::Malformed)
    );
}

#[test]
fn unknown_and_unspecified_enums_fail_closed() {
    let mut unknown = ProcessHostResponseV1 {
        result: Some(process_host_response_v1::Result::Error(
            ProcessProtocolErrorV1 { failure: 99 },
        )),
    };
    assert_eq!(
        decode_host_response_frame(&encode_process_frame(&unknown).unwrap()),
        Err(ProcessFrameError::Malformed)
    );
    unknown.result = Some(process_host_response_v1::Result::Error(
        ProcessProtocolErrorV1 { failure: 0 },
    ));
    assert_eq!(
        decode_host_response_frame(&encode_process_frame(&unknown).unwrap()),
        Err(ProcessFrameError::Malformed)
    );
}

fn framed(payload: &[u8]) -> Vec<u8> {
    let mut frame = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
    frame.extend(payload);
    frame
}
