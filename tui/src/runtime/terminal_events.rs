use std::{
    io,
    sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::Duration,
};

use crossterm::event::{self, Event};
use tokio::sync::mpsc as tokio_mpsc;

const EVENT_CAPACITY: usize = 256;
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const CONTROL_TIMEOUT: Duration = Duration::from_millis(250);

enum Control {
    Pause(SyncSender<()>),
    Resume(SyncSender<()>),
    Shutdown(SyncSender<()>),
}

/// Owns the single terminal-input reader and proves when it is parked.
///
/// Crossterm's asynchronous `EventStream` wakes its worker on drop but does not
/// join it. That permits a released reader to consume terminal-query replies
/// while the full-screen terminal is being reacquired. This component polls on
/// one owned thread and acknowledges pause only after no read is active.
pub(super) struct TerminalEventReader {
    control: Sender<Control>,
    events: tokio_mpsc::Receiver<io::Result<Event>>,
    thread: Option<JoinHandle<()>>,
}

impl TerminalEventReader {
    pub(super) fn start() -> io::Result<Self> {
        let (control_tx, control_rx) = mpsc::channel();
        let (event_tx, event_rx) = tokio_mpsc::channel(EVENT_CAPACITY);
        let thread = thread::Builder::new()
            .name("garive-terminal-events".to_owned())
            .spawn(move || reader_loop(control_rx, event_tx))?;
        Ok(Self {
            control: control_tx,
            events: event_rx,
            thread: Some(thread),
        })
    }

    pub(super) async fn recv(&mut self) -> Option<io::Result<Event>> {
        self.events.recv().await
    }

    pub(super) fn pause(&self) -> io::Result<()> {
        self.command(Control::Pause)
    }

    pub(super) fn resume(&self) -> io::Result<()> {
        self.command(Control::Resume)
    }

    fn command(&self, build: fn(SyncSender<()>) -> Control) -> io::Result<()> {
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        self.control
            .send(build(ack_tx))
            .map_err(|_| reader_stopped())?;
        ack_rx
            .recv_timeout(CONTROL_TIMEOUT)
            .map_err(|_| reader_stopped())
    }
}

impl Drop for TerminalEventReader {
    fn drop(&mut self) {
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        if self.control.send(Control::Shutdown(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(CONTROL_TIMEOUT);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn reader_loop(control: Receiver<Control>, events: tokio_mpsc::Sender<io::Result<Event>>) {
    let mut paused = false;
    let mut pending = None;
    let mut stop_after_pending = false;
    loop {
        match apply_controls(&control, &mut paused) {
            ReaderAction::Continue => {}
            ReaderAction::Shutdown(ack) => {
                let _ = ack.send(());
                break;
            }
            ReaderAction::Disconnected => break,
        }
        if paused {
            match control.recv() {
                Ok(command) => match apply_control(command, &mut paused) {
                    Some(ack) => {
                        let _ = ack.send(());
                        break;
                    }
                    None => continue,
                },
                Err(_) => break,
            }
        }
        if let Some(event) = pending.take() {
            match events.try_send(event) {
                Ok(()) if stop_after_pending => break,
                Ok(()) => continue,
                Err(tokio_mpsc::error::TrySendError::Full(event)) => {
                    pending = Some(event);
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(tokio_mpsc::error::TrySendError::Closed(_)) => break,
            }
        }
        match event::poll(POLL_INTERVAL) {
            Ok(true) => match event::read() {
                Ok(event) => pending = Some(Ok(event)),
                Err(error) => {
                    pending = Some(Err(error));
                    stop_after_pending = true;
                }
            },
            Ok(false) => {}
            Err(error) => {
                pending = Some(Err(error));
                stop_after_pending = true;
            }
        }
    }
}

enum ReaderAction {
    Continue,
    Shutdown(SyncSender<()>),
    Disconnected,
}

fn apply_controls(control: &Receiver<Control>, paused: &mut bool) -> ReaderAction {
    loop {
        match control.try_recv() {
            Ok(command) => {
                if let Some(ack) = apply_control(command, paused) {
                    return ReaderAction::Shutdown(ack);
                }
            }
            Err(TryRecvError::Empty) => return ReaderAction::Continue,
            Err(TryRecvError::Disconnected) => return ReaderAction::Disconnected,
        }
    }
}

fn apply_control(command: Control, paused: &mut bool) -> Option<SyncSender<()>> {
    match command {
        Control::Pause(ack) => {
            *paused = true;
            let _ = ack.send(());
            None
        }
        Control::Resume(ack) => {
            *paused = false;
            let _ = ack.send(());
            None
        }
        Control::Shutdown(ack) => Some(ack),
    }
}

fn reader_stopped() -> io::Error {
    io::Error::other("terminal event reader stopped")
}
