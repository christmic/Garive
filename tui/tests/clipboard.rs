#![allow(dead_code, unused_imports)]

#[path = "../src/runtime/clipboard.rs"]
mod clipboard;

#[test]
fn osc52_copy_is_bounded_and_payload_is_terminal_safe() {
    let sequence = clipboard::sequence("answer\u{1b}]52;c;attack").unwrap();
    assert!(sequence.starts_with("\u{1b}]52;c;"));
    assert!(sequence.ends_with('\u{7}'));
    assert_eq!(sequence.matches('\u{1b}').count(), 1);
    assert!(!sequence.contains("attack"));
    assert!(clipboard::sequence("").is_none());
    assert!(clipboard::sequence(&"x".repeat(65 * 1_024)).is_none());
}
