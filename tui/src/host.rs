use std::{future::Future, pin::Pin};

use garive_host_client::{HostClientError, LiveHostClient};

use crate::application::{HostReadFailure, HostReadResponse};

pub(crate) type HostReadFuture =
    Pin<Box<dyn Future<Output = Result<HostReadResponse, HostReadFailure>> + Send + 'static>>;

pub(crate) trait HostReadPort: Clone + Send + Sync + 'static {
    fn load_definitions(&self) -> HostReadFuture;
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
}

impl From<HostClientError> for HostReadFailure {
    fn from(error: HostClientError) -> Self {
        Self { code: error.code }
    }
}
