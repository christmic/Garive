//! Incremental UTF-8 Server-Sent Events framing independent of JSON semantics.

use crate::MessagesAdapterError;

/// One complete Server-Sent Events frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SseFrame {
    event: Option<String>,
    data: String,
    id: Option<String>,
    retry: Option<u64>,
}

impl SseFrame {
    /// Returns the optional SSE event name.
    pub fn event(&self) -> Option<&str> {
        self.event.as_deref()
    }
    /// Returns data lines joined with a single newline.
    pub fn data(&self) -> &str {
        &self.data
    }
    /// Returns the optional SSE event identifier.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
    /// Returns the optional retry value in milliseconds.
    pub fn retry(&self) -> Option<u64> {
        self.retry
    }
}

/// Stateful SSE framer that accepts arbitrary transport byte chunks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    /// Creates an empty incremental decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes and returns every frame completed by this chunk.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseFrame>, MessagesAdapterError> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();
        while let Some((end, delimiter_length)) = frame_boundary(&self.buffer) {
            let frame_bytes = self.buffer[..end].to_vec();
            self.buffer.drain(..end + delimiter_length);
            if let Some(frame) = parse_frame(&frame_bytes)? {
                frames.push(frame);
            }
        }
        Ok(frames)
    }

    /// Validates that EOF does not leave an incomplete frame.
    pub fn finish(&mut self) -> Result<(), MessagesAdapterError> {
        let trailing_comments = std::str::from_utf8(&self.buffer)
            .map(|text| {
                text.lines()
                    .all(|line| line.trim().is_empty() || line.starts_with(':'))
            })
            .unwrap_or(false);
        if trailing_comments {
            self.buffer.clear();
            Ok(())
        } else {
            Err(MessagesAdapterError::TruncatedStream)
        }
    }
}

fn frame_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes.windows(2).position(|window| window == b"\n\n");
    let crlf = bytes.windows(4).position(|window| window == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(left), Some(right)) if left <= right => Some((left, 2)),
        (Some(_), Some(right)) => Some((right, 4)),
        (Some(left), None) => Some((left, 2)),
        (None, Some(right)) => Some((right, 4)),
        (None, None) => None,
    }
}

fn parse_frame(bytes: &[u8]) -> Result<Option<SseFrame>, MessagesAdapterError> {
    let text = std::str::from_utf8(bytes).map_err(|_| MessagesAdapterError::InvalidSse)?;
    let mut event = None;
    let mut data = Vec::new();
    let mut id = None;
    let mut retry = None;
    for raw_line in text.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = line
            .split_once(':')
            .map(|(field, value)| (field, value.strip_prefix(' ').unwrap_or(value)))
            .unwrap_or((line, ""));
        match field {
            "event" => event = Some(value.to_owned()),
            "data" => data.push(value),
            "id" if !value.contains('\0') => id = Some(value.to_owned()),
            "retry" => {
                retry = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| MessagesAdapterError::InvalidSse)?,
                )
            }
            _ => {}
        }
    }
    if data.is_empty() {
        return Ok(None);
    }
    Ok(Some(SseFrame {
        event,
        data: data.join("\n"),
        id,
        retry,
    }))
}
