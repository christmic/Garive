use std::io::{Read, Write};

use serde::Deserialize;
use serde_json::{json, Value};

const MAX_INPUT_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema_version: u32,
    task_id: String,
    evaluator_rubric: Value,
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    candidate_id: String,
    content: String,
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

fn execute() -> Result<Value, ()> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut input)
        .map_err(|_| ())?;
    if input.is_empty() || input.len() as u64 > MAX_INPUT_BYTES {
        return Err(());
    }
    let request: Request = serde_json::from_slice(&input).map_err(|_| ())?;
    if request.schema_version != 1
        || request.task_id.is_empty()
        || !request.evaluator_rubric.is_object()
        || request.candidates.is_empty()
        || request
            .candidates
            .iter()
            .any(|value| value.candidate_id.is_empty() || value.content.is_empty())
    {
        return Err(());
    }
    let verdicts = request
        .candidates
        .into_iter()
        .map(|candidate| {
            let candidate_id = candidate.candidate_id;
            json!({
                "candidate_id":candidate_id,
                "correct":true,
                "correct_cluster_id":format!("fixture-cluster-{candidate_id}"),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({"schema_version":1,"verdicts":verdicts}))
}
