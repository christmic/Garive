//! Bounded, environment-free Podman CLI process control.

use std::{
    io::Read,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const CONTROL_OUTPUT_BOUND: usize = 65_536;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) struct PodmanCli<'a> {
    executable: &'a Path,
    socket_uri: &'a str,
}

pub(crate) struct CommandOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) truncated: bool,
}

pub(crate) enum AttachCompletion {
    Exited(CommandOutput),
    TimedOut(CommandOutput),
}

impl<'a> PodmanCli<'a> {
    pub(crate) const fn new(executable: &'a Path, socket_uri: &'a str) -> Self {
        Self {
            executable,
            socket_uri,
        }
    }

    pub(crate) fn output(&self, arguments: &[String]) -> Result<CommandOutput, ()> {
        let mut command = self.command();
        command.args(arguments);
        let output = command.output().map_err(|_| ())?;
        let mut remaining = CONTROL_OUTPUT_BOUND;
        let stdout = take_bounded(output.stdout, &mut remaining);
        let stderr = take_bounded(output.stderr, &mut remaining);
        Ok(CommandOutput {
            status: output.status,
            truncated: stdout.1 || stderr.1,
            stdout: stdout.0,
            stderr: stderr.0,
        })
    }

    pub(crate) fn attach(
        &self,
        arguments: &[String],
        output_bound: usize,
        timeout: Duration,
        timeout_cleanup: impl FnOnce() -> Result<(), ()>,
    ) -> Result<AttachCompletion, ()> {
        let mut command = self.command();
        let mut child = command
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| ())?;
        let remaining = Arc::new(AtomicUsize::new(output_bound));
        let truncated = Arc::new(AtomicBool::new(false));
        let stdout = drain(
            child.stdout.take().ok_or(())?,
            Arc::clone(&remaining),
            Arc::clone(&truncated),
        );
        let stderr = drain(
            child.stderr.take().ok_or(())?,
            Arc::clone(&remaining),
            Arc::clone(&truncated),
        );
        let deadline = Instant::now().checked_add(timeout).ok_or(())?;
        let (status, timed_out) = loop {
            if let Some(status) = child.try_wait().map_err(|_| ())? {
                break (status, false);
            }
            if Instant::now() >= deadline {
                timeout_cleanup()?;
                break (child.wait().map_err(|_| ())?, true);
            }
            thread::sleep(POLL_INTERVAL);
        };
        let output = CommandOutput {
            status,
            stdout: stdout.join().map_err(|_| ())?,
            stderr: stderr.join().map_err(|_| ())?,
            truncated: truncated.load(Ordering::Acquire),
        };
        Ok(if timed_out {
            AttachCompletion::TimedOut(output)
        } else {
            AttachCompletion::Exited(output)
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.executable);
        command
            .env_clear()
            .arg("--url")
            .arg(self.socket_uri)
            .stdin(Stdio::null());
        command
    }
}

fn drain(
    mut reader: impl Read + Send + 'static,
    remaining: Arc<AtomicUsize>,
    truncated: Arc<AtomicBool>,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let admitted = remaining
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    Some(value.saturating_sub(count))
                })
                .unwrap_or(0)
                .min(count);
            kept.extend_from_slice(&buffer[..admitted]);
            if admitted < count {
                truncated.store(true, Ordering::Release);
            }
        }
        kept
    })
}

fn take_bounded(mut bytes: Vec<u8>, remaining: &mut usize) -> (Vec<u8>, bool) {
    let admitted = bytes.len().min(*remaining);
    let truncated = admitted < bytes.len();
    bytes.truncate(admitted);
    *remaining -= admitted;
    (bytes, truncated)
}
