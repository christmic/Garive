use std::{future::Future, pin::Pin};

use garive_host_client::{HostClientError, HostClientErrorCode, LiveHostClient};

use crate::application::{
    HostReadFailure, HostReadResponse, SessionPageRequest, SnapshotRead, SnapshotRequest,
};

pub(crate) const PAGE_LIMIT: usize = 100;
const SNAPSHOT_READ_ATTEMPTS: usize = 3;

pub(crate) type HostReadFuture =
    Pin<Box<dyn Future<Output = Result<HostReadResponse, HostReadFailure>> + Send + 'static>>;

pub(crate) trait HostReadPort: Clone + Send + Sync + 'static {
    fn load_definitions(&self) -> HostReadFuture;
    fn load_session_page(&self, request: SessionPageRequest) -> HostReadFuture;
    fn load_snapshot(&self, request: SnapshotRequest) -> HostReadFuture;
}

#[derive(Clone)]
pub(crate) struct LiveHostReadPort {
    client: LiveHostClient,
}

impl LiveHostReadPort {
    pub(crate) fn new(client: LiveHostClient) -> Self {
        Self { client }
    }
}

impl HostReadPort for LiveHostReadPort {
    fn load_definitions(&self) -> HostReadFuture {
        let client = self.client.clone();
        Box::pin(async move {
            client
                .list_agent_definitions()
                .await
                .map(|page| HostReadResponse::Definitions(page.definitions))
                .map_err(HostReadFailure::from)
        })
    }

    fn load_session_page(&self, request: SessionPageRequest) -> HostReadFuture {
        let client = self.client.clone();
        Box::pin(async move {
            client
                .list_sessions(PAGE_LIMIT, request.cursor.as_deref())
                .await
                .map(|page| HostReadResponse::SessionPage {
                    request,
                    sessions: page.sessions,
                    next_before: page.next_before,
                })
                .map_err(HostReadFailure::from)
        })
    }

    fn load_snapshot(&self, request: SnapshotRequest) -> HostReadFuture {
        let client = self.client.clone();
        Box::pin(async move {
            for attempt in 0..SNAPSHOT_READ_ATTEMPTS {
                match read_snapshot(&client, &request).await {
                    Ok(snapshot) => return Ok(HostReadResponse::Snapshot(Box::new(snapshot))),
                    Err(SnapshotReadError::Unstable) if attempt + 1 < SNAPSHOT_READ_ATTEMPTS => {
                        tokio::task::yield_now().await;
                    }
                    Err(SnapshotReadError::Unstable) => return Err(invalid_snapshot()),
                    Err(SnapshotReadError::Host(failure)) => return Err(failure),
                }
            }
            unreachable!("snapshot attempts are non-zero")
        })
    }
}

enum SnapshotReadError {
    Unstable,
    Host(HostReadFailure),
}

async fn read_snapshot(
    client: &LiveHostClient,
    request: &SnapshotRequest,
) -> Result<SnapshotRead, SnapshotReadError> {
    let view = client
        .get_session(&request.session_id)
        .await
        .map_err(|error| SnapshotReadError::Host(error.into()))?;
    let mut items = Vec::new();
    let mut scan = SnapshotScan::default();
    loop {
        let page = client
            .get_timeline(&request.session_id, scan.after, PAGE_LIMIT)
            .await
            .map_err(|error| SnapshotReadError::Host(error.into()))?;
        let complete = scan
            .accept(
                page.observed_max_position,
                page.scanned_through_position,
                page.has_more,
            )
            .map_err(|_| SnapshotReadError::Unstable)?;
        items.extend(page.items);
        if complete {
            break;
        }
    }
    let follow_position = scan
        .finish(view.observed_max_position)
        .map_err(|_| SnapshotReadError::Unstable)?;
    Ok(SnapshotRead {
        request: request.clone(),
        view,
        items,
        follow_position,
    })
}

#[derive(Default)]
struct SnapshotScan {
    after: u64,
    watermark: Option<u64>,
}

impl SnapshotScan {
    fn accept(
        &mut self,
        observed: u64,
        scanned_through: u64,
        has_more: bool,
    ) -> Result<bool, HostReadFailure> {
        match self.watermark {
            Some(expected) if expected != observed => return Err(invalid_snapshot()),
            None => self.watermark = Some(observed),
            Some(_) => {}
        }
        if has_more && scanned_through <= self.after {
            return Err(invalid_snapshot());
        }
        self.after = scanned_through;
        Ok(!has_more)
    }

    fn finish(self, view_watermark: u64) -> Result<u64, HostReadFailure> {
        self.watermark
            .filter(|watermark| *watermark == view_watermark)
            .ok_or_else(invalid_snapshot)
    }
}

const fn invalid_snapshot() -> HostReadFailure {
    HostReadFailure {
        code: HostClientErrorCode::InvalidEvent,
        host_rejected: false,
    }
}

impl From<HostClientError> for HostReadFailure {
    fn from(error: HostClientError) -> Self {
        Self {
            code: error.code,
            host_rejected: error.status.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_scan_freezes_first_watermark_and_completes_exact_view() {
        let mut scan = SnapshotScan::default();
        assert_eq!(scan.accept(41, 20, true), Ok(false));
        assert_eq!(scan.accept(41, 41, false), Ok(true));
        assert_eq!(scan.finish(41), Ok(41));
    }

    #[test]
    fn snapshot_scan_rejects_cross_watermark_page() {
        let mut scan = SnapshotScan::default();
        assert_eq!(scan.accept(41, 20, true), Ok(false));
        assert_eq!(scan.accept(42, 42, false), Err(invalid_snapshot()));
    }

    #[test]
    fn snapshot_scan_rejects_more_without_forward_progress() {
        let mut scan = SnapshotScan::default();
        assert_eq!(scan.accept(41, 20, true), Ok(false));
        assert_eq!(scan.accept(41, 20, true), Err(invalid_snapshot()));
    }

    #[test]
    fn snapshot_scan_rejects_session_view_from_another_watermark() {
        let mut scan = SnapshotScan::default();
        assert_eq!(scan.accept(41, 41, false), Ok(true));
        assert_eq!(scan.finish(42), Err(invalid_snapshot()));
    }
}
