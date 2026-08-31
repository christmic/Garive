use std::{future::Future, pin::Pin};

use garive_host_client::{HostClientError, LiveHostClient};

use crate::application::{HostReadFailure, HostReadResponse, SessionPageRequest};

pub(crate) const PAGE_LIMIT: usize = 100;

pub(crate) type HostReadFuture =
    Pin<Box<dyn Future<Output = Result<HostReadResponse, HostReadFailure>> + Send + 'static>>;

pub(crate) trait HostReadPort: Clone + Send + Sync + 'static {
    fn load_definitions(&self) -> HostReadFuture;
    fn load_session_page(&self, request: SessionPageRequest) -> HostReadFuture;
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
}

impl From<HostClientError> for HostReadFailure {
    fn from(error: HostClientError) -> Self {
        Self { code: error.code }
    }
}
