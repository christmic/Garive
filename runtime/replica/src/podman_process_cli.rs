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
    control_timeout: Duration,
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
    pub(crate) const fn new(
        executable: &'a Path,
        socket_uri: &'a str,
        control_timeout: Duration,
    ) -> Self {
        Self {
            executable,
            socket_uri,
            control_timeout,
        }
    }

    pub(crate) fn output(&self, arguments: &[String]) -> Result<CommandOutput, ()> {
        match self.attach(
            arguments,
            CONTROL_OUTPUT_BOUND,
            self.control_timeout,
            || Ok(()),
        )? {
            AttachCompletion::Exited(output) => Ok(output),
            AttachCompletion::TimedOut(_) => Err(()),
        }
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
                if timeout_cleanup().is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout.join();
                    let _ = stderr.join();
                    return Err(());
                }
                child.kill().map_err(|_| ())?;
                break (child.wait().map_err(|_| ())?, true);
            }
            thread::sleep(POLL_INTERVAL);
        };
        let output = CommandOutput {
            status,
            stdout: stdout.join().map_err(|_| ())??,
            stderr: stderr.join().map_err(|_| ())??,
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
) -> thread::JoinHandle<Result<Vec<u8>, ()>> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => return Err(()),
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
        Ok(kept)
    })
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use super::PodmanCli;

    #[test]
    fn control_timeout_kills_a_flooding_cli_without_pipe_deadlock() {
        let cli = PodmanCli::new(
            Path::new("/usr/bin/yes"),
            "unix:///ignored",
            Duration::from_millis(20),
        );
        let started = std::time::Instant::now();
        assert!(cli.output(&[]).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
