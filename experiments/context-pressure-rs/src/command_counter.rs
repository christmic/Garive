use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use garive_llm::{MediaKind, ModelInputContent, ModelInputItem, ModelRole};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{TokenCounter, TokenCounterDescriptor, TokenCounterFailure};

/// Explicit bounded process configuration for one exact token counter.
pub struct CommandTokenCounterConfig {
    /// Stable counter implementation identity.
    pub counter_id: String,
    /// Exact implementation/model vocabulary revision.
    pub counter_revision: String,
    /// Whether the exact implementation is admitted for publication evidence.
    pub publishable: bool,
    /// Explicit executable path/name; no shell is inserted.
    pub executable: PathBuf,
    /// Exact ordered arguments after the executable.
    pub argv: Vec<String>,
    /// Explicit working directory.
    pub cwd: PathBuf,
    /// Complete environment installed after clearing inherited values.
    pub environment: BTreeMap<String, String>,
    /// Non-zero wall-clock process timeout.
    pub timeout_ms: u64,
    /// Non-zero maximum stdout bytes.
    pub max_stdout_bytes: usize,
    /// Non-zero maximum stderr bytes drained and discarded.
    pub max_stderr_bytes: usize,
}

/// Exact command-backed counter with no ambient configuration lookup.
pub struct CommandTokenCounter {
    descriptor: TokenCounterDescriptor,
    config: CommandTokenCounterConfig,
}

impl CommandTokenCounter {
    /// Validates explicit process bounds and binds canonical configuration.
    pub fn new(config: CommandTokenCounterConfig) -> Result<Self, TokenCounterFailure> {
        if config.executable.as_os_str().is_empty()
            || config.cwd.as_os_str().is_empty()
            || config.timeout_ms == 0
            || config.max_stdout_bytes == 0
            || config.max_stderr_bytes == 0
            || config.argv.iter().any(|value| value.is_empty())
            || config
                .environment
                .iter()
                .any(|(name, _)| name.is_empty() || name.contains('=') || name.contains('\0'))
        {
            return Err(TokenCounterFailure);
        }
        let executable = config.executable.to_string_lossy();
        let cwd = config.cwd.to_string_lossy();
        let digest_input = CanonicalCommandConfig {
            counter_id: &config.counter_id,
            counter_revision: &config.counter_revision,
            executable: executable.as_ref(),
            argv: &config.argv,
            cwd: cwd.as_ref(),
            environment: &config.environment,
            timeout_ms: config.timeout_ms,
            max_stdout_bytes: config.max_stdout_bytes,
            max_stderr_bytes: config.max_stderr_bytes,
        };
        let canonical = serde_jcs::to_vec(&digest_input).map_err(|_| TokenCounterFailure)?;
        let descriptor = TokenCounterDescriptor::new(
            config.counter_id.clone(),
            config.counter_revision.clone(),
            format!("{:x}", Sha256::digest(canonical)),
            config.publishable,
        )
        .ok_or(TokenCounterFailure)?;
        Ok(Self { descriptor, config })
    }
}

impl TokenCounter for CommandTokenCounter {
    fn descriptor(&self) -> &TokenCounterDescriptor {
        &self.descriptor
    }

    fn count_input_tokens(&self, items: &[ModelInputItem]) -> Result<u64, TokenCounterFailure> {
        let request = CounterRequest {
            schema_version: 1,
            input_items: items.iter().map(wire_item).collect(),
        };
        let input = serde_json::to_vec(&request).map_err(|_| TokenCounterFailure)?;
        let mut command = Command::new(&self.config.executable);
        command
            .args(&self.config.argv)
            .current_dir(&self.config.cwd)
            .env_clear()
            .envs(&self.config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| TokenCounterFailure)?;
        child
            .stdin
            .take()
            .ok_or(TokenCounterFailure)?
            .write_all(&input)
            .map_err(|_| TokenCounterFailure)?;
        let stdout = bounded_reader(
            child.stdout.take().ok_or(TokenCounterFailure)?,
            self.config.max_stdout_bytes,
        );
        let stderr = bounded_reader(
            child.stderr.take().ok_or(TokenCounterFailure)?,
            self.config.max_stderr_bytes,
        );
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|_| TokenCounterFailure)? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                let _ = stderr.join();
                return Err(TokenCounterFailure);
            }
            thread::sleep(Duration::from_millis(2));
        };
        let stdout = stdout.join().map_err(|_| TokenCounterFailure)??;
        let stderr = stderr.join().map_err(|_| TokenCounterFailure)??;
        if !status.success()
            || stdout.len() > self.config.max_stdout_bytes
            || stderr.len() > self.config.max_stderr_bytes
        {
            return Err(TokenCounterFailure);
        }
        let response: CounterResponse =
            serde_json::from_slice(&stdout).map_err(|_| TokenCounterFailure)?;
        if response.schema_version != 1 || response.input_tokens == 0 {
            return Err(TokenCounterFailure);
        }
        Ok(response.input_tokens)
    }
}

fn bounded_reader(
    reader: impl Read + Send + 'static,
    maximum: usize,
) -> thread::JoinHandle<Result<Vec<u8>, TokenCounterFailure>> {
    thread::spawn(move || {
        let limit = maximum.checked_add(1).ok_or(TokenCounterFailure)?;
        let mut output = Vec::new();
        reader
            .take(limit as u64)
            .read_to_end(&mut output)
            .map_err(|_| TokenCounterFailure)?;
        Ok(output)
    })
}

fn wire_item(value: &ModelInputItem) -> WireItem<'_> {
    match value {
        ModelInputItem::Message { role, content } => WireItem::Message {
            role: role_name(*role),
            content: content.iter().map(wire_content).collect(),
        },
        ModelInputItem::ToolObservation {
            model_call_id,
            result_json,
        } => WireItem::ToolObservation {
            model_call_id,
            result_json,
        },
        ModelInputItem::ReasoningReference { reference } => {
            WireItem::ReasoningReference { reference }
        }
    }
}

fn wire_content(value: &ModelInputContent) -> WireContent<'_> {
    match value {
        ModelInputContent::Text(text) => WireContent::Text { text },
        ModelInputContent::MediaReference {
            media_kind,
            reference,
            media_type,
        } => WireContent::MediaReference {
            media_kind: media_name(media_kind),
            reference,
            media_type,
        },
    }
}

const fn role_name(value: ModelRole) -> &'static str {
    match value {
        ModelRole::System => "system",
        ModelRole::Developer => "developer",
        ModelRole::User => "user",
        ModelRole::Assistant => "assistant",
    }
}

fn media_name(value: &MediaKind) -> &str {
    match value {
        MediaKind::Image => "image",
        MediaKind::Audio => "audio",
        MediaKind::Video => "video",
        MediaKind::File => "file",
        MediaKind::Other(value) => value,
    }
}

#[derive(Serialize)]
struct CanonicalCommandConfig<'a> {
    counter_id: &'a str,
    counter_revision: &'a str,
    executable: &'a str,
    argv: &'a [String],
    cwd: &'a str,
    environment: &'a BTreeMap<String, String>,
    timeout_ms: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

#[derive(Serialize)]
struct CounterRequest<'a> {
    schema_version: u32,
    input_items: Vec<WireItem<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireItem<'a> {
    Message {
        role: &'static str,
        content: Vec<WireContent<'a>>,
    },
    ToolObservation {
        model_call_id: &'a str,
        result_json: &'a str,
    },
    ReasoningReference {
        reference: &'a str,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WireContent<'a> {
    Text {
        text: &'a str,
    },
    MediaReference {
        media_kind: &'a str,
        reference: &'a str,
        media_type: &'a str,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CounterResponse {
    schema_version: u32,
    input_tokens: u64,
}
