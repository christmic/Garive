use std::io::Cursor;

use garive_adapter_browser_attached::{
    read_frame, write_frame, AttachedFrameError, AttachedLimits,
};
use serde_json::json;

#[test]
fn native_endian_frame_round_trips_one_object() {
    let limits = AttachedLimits::new(1024).expect("limits");
    let value = json!({"kind":"host.challenge","sequence":1});
    let mut bytes = Vec::new();
    write_frame(&mut bytes, limits, &value).expect("write");
    assert_eq!(
        u32::from_ne_bytes(bytes[..4].try_into().expect("prefix")),
        38
    );
    assert_eq!(
        read_frame(&mut Cursor::new(bytes), limits).expect("read"),
        value
    );
}

#[test]
fn empty_oversized_truncated_and_non_object_frames_fail_closed() {
    let limits = AttachedLimits::new(16).expect("limits");
    for (bytes, expected) in [
        (
            0_u32.to_ne_bytes().to_vec(),
            AttachedFrameError::BoundExceeded,
        ),
        (
            17_u32.to_ne_bytes().to_vec(),
            AttachedFrameError::BoundExceeded,
        ),
        (
            [4_u32.to_ne_bytes().as_slice(), b"{}"].concat(),
            AttachedFrameError::Truncated,
        ),
    ] {
        assert_eq!(read_frame(&mut Cursor::new(bytes), limits), Err(expected));
    }
    let mut scalar = Vec::new();
    write_frame(&mut scalar, limits, &json!(true)).expect_err("object only");
    assert_eq!(
        write_frame(&mut Vec::new(), limits, &json!({"too_long":"123456789"})),
        Err(AttachedFrameError::BoundExceeded)
    );
    let duplicate = br#"{"kind":"one","kind":"two"}"#;
    let duplicate = [(duplicate.len() as u32).to_ne_bytes().as_slice(), duplicate].concat();
    assert_eq!(
        read_frame(
            &mut Cursor::new(duplicate),
            AttachedLimits::new(64).expect("limits")
        ),
        Err(AttachedFrameError::InvalidJson)
    );
}
