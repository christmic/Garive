use prost::Message;

use crate::com::garive::process::v1::{
    process_guest_request_v1, process_guest_response_v1, process_host_request_v1,
    process_host_response_v1, ProcessDispatchV1, ProcessGuestRequestV1, ProcessGuestResponseV1,
    ProcessHostRequestV1, ProcessHostResponseV1, ProcessProtocolErrorV1, ProcessServiceStateV1,
    ProcessStatusV1, ProcessWorkspaceModeV1,
};

/// Maximum admitted protobuf payload bytes in one process-protocol frame.
pub const PROCESS_FRAME_MAX_PAYLOAD_BYTES: usize = 1_114_112;

/// Closed, path-free framing failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessFrameError {
    /// Length, protobuf, canonical form, direction, or body is malformed.
    Malformed,
    /// The declared or encoded payload exceeds the fixed frame bound.
    BoundsExceeded,
}

/// Encodes one canonical length-prefixed process-protocol frame.
pub fn encode_process_frame<M: Message>(message: &M) -> Result<Vec<u8>, ProcessFrameError> {
    let payload_length = message.encoded_len();
    if payload_length > PROCESS_FRAME_MAX_PAYLOAD_BYTES {
        return Err(ProcessFrameError::BoundsExceeded);
    }
    let length = u32::try_from(payload_length).map_err(|_| ProcessFrameError::BoundsExceeded)?;
    let mut frame = Vec::with_capacity(4 + payload_length);
    frame.extend(length.to_be_bytes());
    message
        .encode(&mut frame)
        .map_err(|_| ProcessFrameError::Malformed)?;
    Ok(frame)
}

/// Strictly decodes one Runtime-to-XPC host request frame.
pub fn decode_host_request_frame(frame: &[u8]) -> Result<ProcessHostRequestV1, ProcessFrameError> {
    let message: ProcessHostRequestV1 = decode_frame(frame)?;
    if !valid_host_request(&message) {
        return Err(ProcessFrameError::Malformed);
    }
    Ok(message)
}

/// Strictly decodes one XPC-to-Runtime host response frame.
pub fn decode_host_response_frame(
    frame: &[u8],
) -> Result<ProcessHostResponseV1, ProcessFrameError> {
    let message: ProcessHostResponseV1 = decode_frame(frame)?;
    if !valid_host_response(&message) {
        return Err(ProcessFrameError::Malformed);
    }
    Ok(message)
}

/// Strictly decodes one XPC-service-to-guest request frame.
pub fn decode_guest_request_frame(
    frame: &[u8],
) -> Result<ProcessGuestRequestV1, ProcessFrameError> {
    let message: ProcessGuestRequestV1 = decode_frame(frame)?;
    if !valid_guest_request(&message) {
        return Err(ProcessFrameError::Malformed);
    }
    Ok(message)
}

/// Strictly decodes one guest-to-XPC-service response frame.
pub fn decode_guest_response_frame(
    frame: &[u8],
) -> Result<ProcessGuestResponseV1, ProcessFrameError> {
    let message: ProcessGuestResponseV1 = decode_frame(frame)?;
    if !valid_guest_response(&message) {
        return Err(ProcessFrameError::Malformed);
    }
    Ok(message)
}

fn valid_host_request(value: &ProcessHostRequestV1) -> bool {
    match value.command.as_ref() {
        Some(process_host_request_v1::Command::Preflight(value))
        | Some(process_host_request_v1::Command::Start(value)) => valid_dispatch(value),
        Some(process_host_request_v1::Command::Query(value))
        | Some(process_host_request_v1::Command::Terminate(value)) => value.identity.is_some(),
        Some(process_host_request_v1::Command::Acknowledge(value)) => {
            value.identity.is_some() && value.receipt_digest.len() == 32
        }
        None => false,
    }
}

fn valid_host_response(value: &ProcessHostResponseV1) -> bool {
    match value.result.as_ref() {
        Some(process_host_response_v1::Result::Preflighted(value)) => value.identity.is_some(),
        Some(process_host_response_v1::Result::Status(value)) => valid_status(value),
        Some(process_host_response_v1::Result::Terminal(value)) => valid_terminal_shape(value),
        Some(process_host_response_v1::Result::Error(value)) => valid_protocol_error(value),
        None => false,
    }
}

fn valid_guest_request(value: &ProcessGuestRequestV1) -> bool {
    match value.command.as_ref() {
        Some(process_guest_request_v1::Command::Hello(value)) => {
            value.identity.is_some() && value.challenge.len() == 32
        }
        Some(process_guest_request_v1::Command::Execute(value)) => {
            value.workload.as_ref().is_some_and(|workload| {
                value.identity.is_some() && valid_workspace_mode(workload.workspace_mode)
            })
        }
        Some(process_guest_request_v1::Command::Terminate(value)) => value.identity.is_some(),
        None => false,
    }
}

fn valid_guest_response(value: &ProcessGuestResponseV1) -> bool {
    match value.result.as_ref() {
        Some(process_guest_response_v1::Result::Ready(value)) => {
            value.identity.is_some()
                && value.challenge.len() == 32
                && !value.guest_agent_revision.is_empty()
        }
        Some(process_guest_response_v1::Result::Terminal(value)) => valid_terminal_shape(value),
        Some(process_guest_response_v1::Result::Error(value)) => valid_protocol_error(value),
        None => false,
    }
}

fn valid_dispatch(value: &ProcessDispatchV1) -> bool {
    value.identity.is_some()
        && value
            .vm_plan
            .as_ref()
            .is_some_and(|plan| valid_workspace_mode(plan.workspace_mode))
        && value
            .workload
            .as_ref()
            .is_some_and(|workload| valid_workspace_mode(workload.workspace_mode))
}

fn valid_status(value: &ProcessStatusV1) -> bool {
    let Ok(state) = ProcessServiceStateV1::try_from(value.state) else {
        return false;
    };
    value.identity.is_some()
        && match state {
            ProcessServiceStateV1::ProcessServiceStateTerminalRetained => {
                value.terminal.as_ref().is_some_and(valid_terminal_shape)
            }
            ProcessServiceStateV1::ProcessServiceStateAbsent
            | ProcessServiceStateV1::ProcessServiceStateStarting
            | ProcessServiceStateV1::ProcessServiceStateRunning => value.terminal.is_none(),
            ProcessServiceStateV1::ProcessServiceStateUnspecified => false,
        }
}

fn valid_terminal_shape(value: &crate::com::garive::process::v1::ProcessTerminalReceiptV1) -> bool {
    value.identity.is_some()
        && value
            .exit
            .as_ref()
            .is_some_and(|exit| exit.classification.is_some())
}

fn valid_protocol_error(value: &ProcessProtocolErrorV1) -> bool {
    crate::com::garive::process::v1::ProcessProtocolFailureV1::try_from(value.failure)
        .is_ok_and(|failure| {
            failure
                != crate::com::garive::process::v1::ProcessProtocolFailureV1::ProcessProtocolFailureUnspecified
        })
}

fn valid_workspace_mode(value: i32) -> bool {
    ProcessWorkspaceModeV1::try_from(value).is_ok_and(|mode| {
        matches!(
            mode,
            ProcessWorkspaceModeV1::ProcessWorkspaceModeReadOnly
                | ProcessWorkspaceModeV1::ProcessWorkspaceModeReadWrite
        )
    })
}

fn decode_frame<M: Message + Default>(frame: &[u8]) -> Result<M, ProcessFrameError> {
    let prefix: [u8; 4] = frame
        .get(..4)
        .ok_or(ProcessFrameError::Malformed)?
        .try_into()
        .map_err(|_| ProcessFrameError::Malformed)?;
    let declared = u32::from_be_bytes(prefix) as usize;
    if declared > PROCESS_FRAME_MAX_PAYLOAD_BYTES {
        return Err(ProcessFrameError::BoundsExceeded);
    }
    let payload = frame.get(4..).ok_or(ProcessFrameError::Malformed)?;
    if payload.len() != declared {
        return Err(ProcessFrameError::Malformed);
    }
    let message = M::decode(payload).map_err(|_| ProcessFrameError::Malformed)?;
    if message.encode_to_vec() != payload {
        return Err(ProcessFrameError::Malformed);
    }
    Ok(message)
}
