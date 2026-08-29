use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{CreativityBaselineError, CreativityBaselineErrorCode, ExperimentPortDescriptor};

/// Explicit bounded process configuration for one CR-A command port.
pub struct CommandPortConfig {
    /// Stable implementation identity.
    pub implementation_id: String,
    /// Exact implementation/configuration vocabulary revision.
    pub implementation_revision: String,
    /// Must remain false for the CR-A command implementation.
    pub publishable: bool,
    /// Explicit executable path/name; no shell is inserted.
    pub executable: PathBuf,
    /// Exact ordered arguments after the executable.
    pub argv: Vec<String>,
    /// Explicit working directory.
    pub cwd: PathBuf,
    /// Complete environment installed after clearing inherited values.
    pub environment: BTreeMap<String, String>,
    /// Non-zero wall-clock timeout for one attempt.
    pub timeout_ms: u64,
    /// Non-zero maximum stdout bytes.
    pub max_stdout_bytes: usize,
    /// Non-zero maximum stderr bytes drained and discarded.
    pub max_stderr_bytes: usize,
}

pub(crate) struct CommandProcess {
    descriptor: ExperimentPortDescriptor,
    config: CommandPortConfig,
}

impl CommandProcess {
    pub(crate) fn new(
        kind: &'static str,
        config: CommandPortConfig,
    ) -> Result<Self, CreativityBaselineError> {
        if config.publishable
            || config.executable.as_os_str().is_empty()
            || config.cwd.as_os_str().is_empty()
            || config.timeout_ms == 0
            || config.max_stdout_bytes == 0
            || config.max_stderr_bytes == 0
            || config.argv.iter().any(|value| value.is_empty())
            || config.environment.iter().any(|(name, value)| {
                name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0')
            })
        {
            return Err(error());
        }
        let canonical = serde_jcs::to_vec(&CanonicalProcessConfig {
            kind,
            implementation_id: &config.implementation_id,
            implementation_revision: &config.implementation_revision,
            executable: &config.executable.to_string_lossy(),
            argv: &config.argv,
            cwd: &config.cwd.to_string_lossy(),
            environment: &config.environment,
            timeout_ms: config.timeout_ms,
            max_stdout_bytes: config.max_stdout_bytes,
            max_stderr_bytes: config.max_stderr_bytes,
        })
        .map_err(|_| error())?;
        let descriptor = ExperimentPortDescriptor::new(
            config.implementation_id.clone(),
            config.implementation_revision.clone(),
            format!("{:x}", Sha256::digest(canonical)),
            false,
        )
        .ok_or_else(error)?;
        Ok(Self { descriptor, config })
    }

    pub(crate) const fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.descriptor
    }

    pub(crate) fn execute(
        &self,
        input: &impl Serialize,
    ) -> Result<Vec<u8>, CreativityBaselineError> {
        let input = serde_json::to_vec(input).map_err(|_| error())?;
        let mut child = Command::new(&self.config.executable)
            .args(&self.config.argv)
            .current_dir(&self.config.cwd)
            .env_clear()
            .envs(&self.config.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| error())?;
        child
            .stdin
            .take()
            .ok_or_else(error)?
            .write_all(&input)
            .map_err(|_| error())?;
        let stdout = bounded_reader(
            child.stdout.take().ok_or_else(error)?,
            self.config.max_stdout_bytes,
        );
        let stderr = bounded_reader(
            child.stderr.take().ok_or_else(error)?,
            self.config.max_stderr_bytes,
        );
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|_| error())? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                let _ = stderr.join();
                return Err(error());
            }
            thread::sleep(Duration::from_millis(2));
        };
        let stdout = stdout.join().map_err(|_| error())??;
        let stderr = stderr.join().map_err(|_| error())??;
        if !status.success()
            || stdout.len() > self.config.max_stdout_bytes
            || stderr.len() > self.config.max_stderr_bytes
        {
            return Err(error());
        }
        Ok(stdout)
    }
}

fn bounded_reader(
    reader: impl Read + Send + 'static,
    maximum: usize,
) -> thread::JoinHandle<Result<Vec<u8>, CreativityBaselineError>> {
    thread::spawn(move || {
        let limit = maximum.checked_add(1).ok_or_else(error)?;
        let mut output = Vec::new();
        reader
            .take(limit as u64)
            .read_to_end(&mut output)
            .map_err(|_| error())?;
        Ok(output)
    })
}

#[derive(Serialize)]
struct CanonicalProcessConfig<'a> {
    kind: &'static str,
    implementation_id: &'a str,
    implementation_revision: &'a str,
    executable: &'a str,
    argv: &'a [String],
    cwd: &'a str,
    environment: &'a BTreeMap<String, String>,
    timeout_ms: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

fn error() -> CreativityBaselineError {
    CreativityBaselineError::new(CreativityBaselineErrorCode::InvalidPort)
}
