use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;

/// Explicit bounded Git process values for publication provenance.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitAttestationConfig {
    executable: PathBuf,
    repository_path: PathBuf,
    timeout_ms: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

/// Content-free failure proving a revision is not publication-ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GitAttestationFailure;

/// Verifies exact `HEAD` and an empty porcelain status with bounded processes.
pub fn attest_clean_revision(
    config: &GitAttestationConfig,
    expected_revision: &str,
) -> Result<(), GitAttestationFailure> {
    if config.executable.as_os_str().is_empty()
        || config.repository_path.as_os_str().is_empty()
        || config.timeout_ms == 0
        || config.max_stdout_bytes == 0
        || config.max_stderr_bytes == 0
        || expected_revision.is_empty()
        || expected_revision.len() > 256
    {
        return Err(GitAttestationFailure);
    }
    let head = run(
        config,
        &["rev-parse", "--verify", "HEAD"],
        config.max_stdout_bytes,
    )?;
    let head = std::str::from_utf8(&head)
        .map_err(|_| GitAttestationFailure)?
        .trim_end_matches(['\r', '\n']);
    if head != expected_revision
        || head.len() < 40
        || !head.bytes().all(|value| value.is_ascii_hexdigit())
    {
        return Err(GitAttestationFailure);
    }
    let status = run(
        config,
        &["status", "--porcelain=v1", "--untracked-files=all"],
        config.max_stdout_bytes,
    )?;
    if !status.is_empty() {
        return Err(GitAttestationFailure);
    }
    Ok(())
}

fn run(
    config: &GitAttestationConfig,
    operation: &[&str],
    maximum: usize,
) -> Result<Vec<u8>, GitAttestationFailure> {
    let mut child = Command::new(&config.executable)
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args(operation)
        .current_dir(&config.repository_path)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| GitAttestationFailure)?;
    let stdout = bounded_reader(child.stdout.take().ok_or(GitAttestationFailure)?, maximum);
    let stderr = bounded_reader(
        child.stderr.take().ok_or(GitAttestationFailure)?,
        config.max_stderr_bytes,
    );
    let deadline = Instant::now() + Duration::from_millis(config.timeout_ms);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|_| GitAttestationFailure)? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err(GitAttestationFailure);
        }
        thread::sleep(Duration::from_millis(2));
    };
    let stdout = stdout.join().map_err(|_| GitAttestationFailure)??;
    let stderr = stderr.join().map_err(|_| GitAttestationFailure)??;
    if !status.success() || stdout.len() > maximum || stderr.len() > config.max_stderr_bytes {
        return Err(GitAttestationFailure);
    }
    Ok(stdout)
}

fn bounded_reader(
    reader: impl Read + Send + 'static,
    maximum: usize,
) -> thread::JoinHandle<Result<Vec<u8>, GitAttestationFailure>> {
    thread::spawn(move || {
        let limit = maximum.checked_add(1).ok_or(GitAttestationFailure)?;
        let mut output = Vec::new();
        reader
            .take(limit as u64)
            .read_to_end(&mut output)
            .map_err(|_| GitAttestationFailure)?;
        Ok(output)
    })
}
