use std::io::{Read, Write};

use serde::Deserialize;
use serde_json::json;

const MAX_INPUT_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    task_id: String,
    arm: String,
    prompt: String,
    seed: u64,
    max_candidates: u64,
    max_candidate_utf8_bytes: usize,
    max_total_candidate_utf8_bytes: usize,
}

fn main() -> std::process::ExitCode {
    match execute() {
        Ok(value) => {
            if std::io::stdout()
                .write_all(&serde_json::to_vec(&value).expect("fixed output"))
                .is_ok()
            {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::from(2)
            }
        }
        Err(()) => std::process::ExitCode::from(2),
    }
}

fn execute() -> Result<serde_json::Value, ()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut input)
        .map_err(|_| ())?;
    if input.is_empty() || input.len() as u64 > MAX_INPUT_BYTES {
        return Err(());
    }
    let request: Request = serde_json::from_slice(&input).map_err(|_| ())?;
    let count = match request.arm.as_str() {
        "control" if request.max_candidates == 1 => 1,
        "bounded_alternatives" if request.max_candidates >= 2 => request.max_candidates.min(3),
        _ => return Err(()),
    };
    if request.schema_version != 1
        || request.task_id.is_empty()
        || request.prompt.is_empty()
        || request.max_candidate_utf8_bytes == 0
        || request.max_total_candidate_utf8_bytes == 0
    {
        return Err(());
    }
    let candidates = (0..count)
        .map(|index| {
            json!({
                "candidate_id":format!("candidate-{index}"),
                "content":format!("fixture alternative {index} for seed {}", request.seed),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version":1,
        "candidates":candidates,
        "selected_candidate_id":"candidate-0",
    }))
}
