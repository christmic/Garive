use prost::Message;

use crate::com::garive::process::v1::{
    ProcessGuestRequestV1, ProcessGuestResponseV1, ProcessHostRequestV1, ProcessHostResponseV1,
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
    message
        .command
        .as_ref()
        .ok_or(ProcessFrameError::Malformed)?;
    Ok(message)
}

/// Strictly decodes one XPC-to-Runtime host response frame.
pub fn decode_host_response_frame(
    frame: &[u8],
) -> Result<ProcessHostResponseV1, ProcessFrameError> {
    let message: ProcessHostResponseV1 = decode_frame(frame)?;
    message
        .result
        .as_ref()
        .ok_or(ProcessFrameError::Malformed)?;
    Ok(message)
}

/// Strictly decodes one XPC-service-to-guest request frame.
pub fn decode_guest_request_frame(
    frame: &[u8],
) -> Result<ProcessGuestRequestV1, ProcessFrameError> {
    let message: ProcessGuestRequestV1 = decode_frame(frame)?;
    message
        .command
        .as_ref()
        .ok_or(ProcessFrameError::Malformed)?;
    Ok(message)
}

/// Strictly decodes one guest-to-XPC-service response frame.
pub fn decode_guest_response_frame(
    frame: &[u8],
) -> Result<ProcessGuestResponseV1, ProcessFrameError> {
    let message: ProcessGuestResponseV1 = decode_frame(frame)?;
    message
        .result
        .as_ref()
        .ok_or(ProcessFrameError::Malformed)?;
    Ok(message)
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
