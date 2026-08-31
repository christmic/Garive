use std::{future::Future, panic, pin::Pin};

use crate::application::{
    PendingMutationDraft, PendingMutationKind, PersistedPendingIdentity, PersistenceFailure,
};

use super::{PendingCommand, PendingKind, StateError, StateStore};

pub(crate) type PersistenceFuture =
    Pin<Box<dyn Future<Output = Result<PersistedPendingIdentity, PersistenceFailure>> + Send>>;

pub(crate) trait PersistencePort: Clone + Send + Sync + 'static {
    fn persist_pending(&self, draft: PendingMutationDraft) -> PersistenceFuture;
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct AsyncStateStore {
    store: StateStore,
}

#[allow(dead_code)]
impl AsyncStateStore {
    pub(crate) fn new(store: StateStore) -> Self {
        Self { store }
    }
}

impl PersistencePort for AsyncStateStore {
    fn persist_pending(&self, draft: PendingMutationDraft) -> PersistenceFuture {
        let store = self.store.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || persist(&store, draft)).await {
                Ok(result) => result.map_err(map_failure),
                Err(error) if error.is_panic() => panic::resume_unwind(error.into_panic()),
                Err(_) => Err(PersistenceFailure::Unavailable),
            }
        })
    }
}

#[allow(dead_code)]
fn persist(
    store: &StateStore,
    draft: PendingMutationDraft,
) -> Result<PersistedPendingIdentity, StateError> {
    let kind = match draft.kind {
        PendingMutationKind::CreateSession => PendingKind::CreateSession,
        PendingMutationKind::StartTurn => PendingKind::StartTurn,
        PendingMutationKind::CancelTurn => PendingKind::CancelTurn,
        PendingMutationKind::ContinueTurn => PendingKind::ContinueTurn,
    };
    let command = PendingCommand {
        schema_version: 1,
        command_id: draft.command_id,
        kind,
        session_id: draft.session_id,
        turn_id: draft.turn_id,
        suspension_id: draft.suspension_id,
        expected_session_version: draft.expected_session_version,
        requested_through_position: draft.requested_through_position,
        request_payload: draft.request_payload,
        request_digest: String::new(),
        created_at: draft.created_at,
    }
    .seal()?;
    let identity = PersistedPendingIdentity {
        command_id: command.command_id.clone(),
        request_digest: command.request_digest.clone(),
    };
    store.save_pending(&command)?;
    Ok(identity)
}

#[allow(dead_code)]
fn map_failure(error: StateError) -> PersistenceFailure {
    match error {
        StateError::Unavailable => PersistenceFailure::Unavailable,
        StateError::UnsafePermissions => PersistenceFailure::UnsafePermissions,
        StateError::InvalidData => PersistenceFailure::InvalidData,
        StateError::Conflict => PersistenceFailure::Conflict,
    }
}
