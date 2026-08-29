use std::io::{Read, Write};

use serde::Deserialize;
use serde_json::json;

const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    input_items: Vec<InputItem>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum InputItem {
    Message {
        role: String,
        content: Vec<Content>,
    },
    ToolObservation {
        model_call_id: String,
        result_json: String,
    },
    ReasoningReference {
        reference: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Content {
    Text {
        text: String,
    },
    MediaReference {
        media_kind: String,
        reference: String,
        media_type: String,
    },
}

fn main() -> std::process::ExitCode {
    match execute() {
        Ok(tokens) => {
            let output = serde_json::to_vec(&json!({
                "schema_version":1,
                "input_tokens":tokens,
            }))
            .expect("fixed response");
            if std::io::stdout().write_all(&output).is_ok() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::from(2)
            }
        }
        Err(()) => std::process::ExitCode::from(2),
    }
}

fn execute() -> Result<u64, ()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut input)
        .map_err(|_| ())?;
    if input.is_empty() || input.len() as u64 > MAX_INPUT_BYTES {
        return Err(());
    }
    let request: Request = serde_json::from_slice(&input).map_err(|_| ())?;
    if request.schema_version != 1 || request.input_items.is_empty() {
        return Err(());
    }
    let mut bytes = 0_u64;
    for item in request.input_items {
        let cost = match item {
            InputItem::Message { role, content } => {
                if !matches!(role.as_str(), "system" | "developer" | "user" | "assistant")
                    || content.is_empty()
                {
                    return Err(());
                }
                content.into_iter().try_fold(0_u64, |total, part| {
                    let value = match part {
                        Content::Text { text } => text.len() as u64,
                        Content::MediaReference {
                            media_kind,
                            reference,
                            media_type,
                        } => (media_kind.len() + reference.len() + media_type.len()) as u64,
                    };
                    total.checked_add(value).ok_or(())
                })?
            }
            InputItem::ToolObservation {
                model_call_id,
                result_json,
            } => (model_call_id.len() + result_json.len()) as u64,
            InputItem::ReasoningReference { reference } => reference.len() as u64,
        };
        bytes = bytes.checked_add(cost).ok_or(())?;
    }
    bytes
        .checked_add(3)
        .map(|value| value / 4)
        .filter(|value| *value != 0)
        .ok_or(())
}
