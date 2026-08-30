use std::io::{self, Write};

use base64::{engine::general_purpose::STANDARD, Engine};

const MAX_COPY_BYTES: usize = 64 * 1_024;

pub(super) fn copy(value: &str) -> io::Result<()> {
    let sequence = sequence(value).ok_or_else(|| io::Error::other("copy bound exceeded"))?;
    let mut stderr = io::stderr().lock();
    stderr.write_all(sequence.as_bytes())?;
    stderr.flush()
}

pub(super) fn sequence(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > MAX_COPY_BYTES {
        return None;
    }
    Some(format!("\u{1b}]52;c;{}\u{7}", STANDARD.encode(value)))
}
