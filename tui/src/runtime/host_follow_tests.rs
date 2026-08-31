use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use super::*;

struct PendingFollow {
    polled: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
}

impl Future for PendingFollow {
    type Output = Result<(), garive_host_client::HostClientError>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        self.polled.store(true, Ordering::SeqCst);
        Poll::Pending
    }
}

impl Drop for PendingFollow {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn aborting_relay_drops_the_actual_follow_future() {
    let polled = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let follow = PendingFollow {
        polled: polled.clone(),
        dropped: dropped.clone(),
    };
    let (_event_sender, events) = mpsc::channel::<HostEvent>(1);
    let (message_sender, _messages) = mpsc::channel(1);
    let relay = tokio::spawn(async move {
        relay_follow(events, follow, &message_sender, |_| unreachable!()).await
    });

    tokio::task::yield_now().await;
    assert!(polled.load(Ordering::SeqCst));
    relay.abort();
    assert!(relay.await.is_err());
    assert!(dropped.load(Ordering::SeqCst));
}
