use std::{future::Future, time::Duration};

use crossterm::event::Event;
use tokio::time::Instant;

use crate::TuiError;

const LOCAL_BUDGET: u8 = 8;
const RESIZE_WINDOW: Duration = Duration::from_millis(16);

#[cfg(unix)]
pub(in crate::runtime) struct ShutdownSignal {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl ShutdownSignal {
    pub(in crate::runtime) fn new() -> Result<Self, TuiError> {
        use tokio::signal::unix::{signal, SignalKind};

        Ok(Self {
            interrupt: signal(SignalKind::interrupt()).map_err(|_| TuiError::TerminalIo)?,
            terminate: signal(SignalKind::terminate()).map_err(|_| TuiError::TerminalIo)?,
        })
    }

    pub(in crate::runtime) async fn recv(&mut self) -> i32 {
        tokio::select! {
            _ = self.interrupt.recv() => 2,
            _ = self.terminate.recv() => 15,
        }
    }
}

#[cfg(windows)]
pub(in crate::runtime) struct ShutdownSignal {
    interrupt: tokio::signal::windows::CtrlC,
}

#[cfg(windows)]
impl ShutdownSignal {
    pub(in crate::runtime) fn new() -> Result<Self, TuiError> {
        Ok(Self {
            interrupt: tokio::signal::windows::ctrl_c().map_err(|_| TuiError::TerminalIo)?,
        })
    }

    pub(in crate::runtime) async fn recv(&mut self) -> i32 {
        let _ = self.interrupt.recv().await;
        2
    }
}

pub(super) enum Scheduled<S, T, A, H> {
    Shutdown(S),
    Terminal(T),
    Action(A),
    Motion,
    LiveFrame,
    Host(H),
    ResizeDeadline,
}

#[derive(Default)]
pub(super) struct FairScheduler {
    local_since_host: u8,
}

impl FairScheduler {
    fn host_preferred(&self) -> bool {
        self.local_since_host >= LOCAL_BUDGET
    }

    fn record_local(&mut self) {
        self.local_since_host = self.local_since_host.saturating_add(1);
    }

    fn record_host(&mut self) {
        self.local_since_host = 0;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn select_next<SF, TF, AF, MF, LF, HF, RF, S, T, A, H>(
    scheduler: &mut FairScheduler,
    shutdown: SF,
    terminal: TF,
    action: AF,
    motion: MF,
    live_frame: LF,
    host: HF,
    resize_deadline: RF,
    motion_enabled: bool,
    live_frame_enabled: bool,
    resize_pending: bool,
) -> Scheduled<S, T, A, H>
where
    SF: Future<Output = S>,
    TF: Future<Output = T>,
    AF: Future<Output = A>,
    MF: Future,
    LF: Future,
    HF: Future<Output = H>,
    RF: Future,
{
    tokio::pin!(
        shutdown,
        terminal,
        action,
        motion,
        live_frame,
        host,
        resize_deadline
    );
    let selected = if scheduler.host_preferred() {
        tokio::select! {
            biased;
            value = &mut shutdown => Scheduled::Shutdown(value),
            _ = &mut resize_deadline, if resize_pending => Scheduled::ResizeDeadline,
            _ = &mut live_frame, if live_frame_enabled => Scheduled::LiveFrame,
            _ = &mut motion, if motion_enabled => Scheduled::Motion,
            value = &mut host => Scheduled::Host(value),
            value = &mut terminal => Scheduled::Terminal(value),
            value = &mut action => Scheduled::Action(value),
        }
    } else {
        tokio::select! {
            biased;
            value = &mut shutdown => Scheduled::Shutdown(value),
            _ = &mut resize_deadline, if resize_pending => Scheduled::ResizeDeadline,
            value = &mut terminal => Scheduled::Terminal(value),
            value = &mut action => Scheduled::Action(value),
            _ = &mut live_frame, if live_frame_enabled => Scheduled::LiveFrame,
            _ = &mut motion, if motion_enabled => Scheduled::Motion,
            value = &mut host => Scheduled::Host(value),
        }
    };
    match selected {
        Scheduled::Terminal(_) | Scheduled::Action(_) => scheduler.record_local(),
        Scheduled::Host(_) => scheduler.record_host(),
        _ => {}
    }
    selected
}

#[derive(Default)]
pub(super) struct ResizeCoalescer {
    latest: Option<(u16, u16)>,
    deadline: Option<Instant>,
}

impl ResizeCoalescer {
    pub(super) fn push(&mut self, width: u16, height: u16, now: Instant) {
        self.latest = Some((width, height));
        self.deadline.get_or_insert(now + RESIZE_WINDOW);
    }

    pub(super) fn is_pending(&self) -> bool {
        self.latest.is_some()
    }

    pub(super) async fn wait(&self) {
        match self.deadline {
            Some(deadline) => tokio::time::sleep_until(deadline).await,
            None => std::future::pending().await,
        }
    }

    pub(super) fn take(&mut self) -> Option<Event> {
        self.deadline = None;
        self.latest
            .take()
            .map(|(width, height)| Event::Resize(width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{pending, ready};

    async fn ready_local_and_host(scheduler: &mut FairScheduler) -> &'static str {
        match select_next(
            scheduler,
            pending::<()>(),
            ready("local"),
            pending::<()>(),
            pending::<()>(),
            pending::<()>(),
            ready("host"),
            pending::<()>(),
            false,
            false,
            false,
        )
        .await
        {
            Scheduled::Terminal(value) | Scheduled::Host(value) => value,
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn continuous_local_work_cannot_starve_host() {
        let mut scheduler = FairScheduler::default();
        let mut observed = Vec::new();
        for _ in 0..18 {
            observed.push(ready_local_and_host(&mut scheduler).await);
        }
        assert_eq!(observed[8], "host");
        assert_eq!(observed[17], "host");
        assert_eq!(observed.iter().filter(|item| **item == "host").count(), 2);
    }

    #[tokio::test]
    async fn preferred_host_delays_ready_terminal_by_at_most_one_host() {
        let mut scheduler = FairScheduler::default();
        for _ in 0..LOCAL_BUDGET {
            scheduler.record_local();
        }
        assert_eq!(ready_local_and_host(&mut scheduler).await, "host");
        assert_eq!(ready_local_and_host(&mut scheduler).await, "local");
    }

    #[test]
    fn resize_window_keeps_the_final_size_and_fixed_first_deadline() {
        let start = Instant::now();
        let mut coalescer = ResizeCoalescer::default();
        coalescer.push(80, 20, start);
        let deadline = coalescer.deadline;
        assert_eq!(deadline, Some(start + RESIZE_WINDOW));
        coalescer.push(120, 40, start + Duration::from_millis(15));
        assert_eq!(coalescer.deadline, deadline);
        assert_eq!(coalescer.take(), Some(Event::Resize(120, 40)));
        assert!(!coalescer.is_pending());
    }
}
