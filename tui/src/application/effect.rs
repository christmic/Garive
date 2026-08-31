use std::collections::BTreeMap;

use garive_host_client::{
    AgentDefinitionSummary, HostClientErrorCode, SessionSummary, SessionView, TurnTimelineItem,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct EffectId(u64);

impl EffectId {
    fn next(previous: u64) -> Option<(Self, u64)> {
        let value = previous.checked_add(1)?;
        Some((Self(value), value))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AppGeneration(pub(crate) u64);

impl AppGeneration {
    pub(crate) const fn initial() -> Self {
        Self(1)
    }
}

impl Default for AppGeneration {
    fn default() -> Self {
        Self::initial()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct EffectContext {
    pub(crate) effect_id: EffectId,
    pub(crate) issued_generation: AppGeneration,
    pub(crate) session_id: Option<String>,
    pub(crate) request_digest: Option<String>,
}

impl std::fmt::Debug for EffectContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EffectContext")
            .field("effect_id", &self.effect_id)
            .field("issued_generation", &self.issued_generation)
            .field("has_session", &self.session_id.is_some())
            .field("has_request_digest", &self.request_digest.is_some())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionPagePurpose {
    Replace,
    CatalogRefresh,
    Append,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SessionPageRequest {
    pub(crate) cursor: Option<String>,
    pub(crate) purpose: SessionPagePurpose,
}

impl SessionPageRequest {
    pub(crate) fn identity_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"garive-tui-session-page-v1\0");
        hasher.update(match self.purpose {
            SessionPagePurpose::Replace => b"replace".as_slice(),
            SessionPagePurpose::CatalogRefresh => b"catalog-refresh".as_slice(),
            SessionPagePurpose::Append => b"append".as_slice(),
        });
        hasher.update(b"\0");
        if let Some(cursor) = &self.cursor {
            hasher.update(cursor.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

impl std::fmt::Debug for SessionPageRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionPageRequest")
            .field("purpose", &self.purpose)
            .field("has_cursor", &self.cursor.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionPageOwner {
    pub(crate) context: EffectContext,
    pub(crate) request: SessionPageRequest,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SnapshotRequest {
    pub(crate) session_id: String,
}

impl std::fmt::Debug for SnapshotRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnapshotRequest")
            .field("has_session", &!self.session_id.is_empty())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SnapshotRead {
    pub(crate) request: SnapshotRequest,
    pub(crate) view: SessionView,
    pub(crate) items: Vec<TurnTimelineItem>,
    pub(crate) follow_position: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EffectKind {
    Exit,
    #[allow(dead_code)]
    LoadDefinitions,
    LoadSessionPage {
        request: SessionPageRequest,
    },
    #[allow(dead_code)]
    LoadSnapshot {
        request: SnapshotRequest,
    },
    #[allow(dead_code)]
    PersistPending {
        draft: PendingMutationDraft,
    },
    StartTurn {
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    },
    CreateSession {
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    },
    CancelTurn {
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
    },
    ContinueTurn {
        draft: PendingMutationDraft,
        identity: PersistedPendingIdentity,
        schema_digest: String,
        host_allowed: bool,
    },
    PersistContinuation {
        draft: PendingMutationDraft,
        schema_digest: String,
    },
}

impl EffectKind {
    pub(crate) fn tag(&self) -> EffectTag {
        match self {
            Self::Exit => EffectTag::Exit,
            Self::LoadDefinitions => EffectTag::LoadDefinitions,
            Self::LoadSessionPage { .. } => EffectTag::LoadSessionPage,
            Self::LoadSnapshot { .. } => EffectTag::LoadSnapshot,
            Self::PersistPending { .. } => EffectTag::PersistPending,
            Self::StartTurn { .. } => EffectTag::StartTurn,
            Self::CreateSession { .. } => EffectTag::CreateSession,
            Self::CancelTurn { .. } => EffectTag::CancelTurn,
            Self::ContinueTurn { .. } => EffectTag::ContinueTurn,
            Self::PersistContinuation { .. } => EffectTag::PersistContinuation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PendingMutationKind {
    CreateSession,
    CancelTurn,
    ContinueTurn,
    StartTurn,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct PendingMutationDraft {
    pub(crate) command_id: String,
    pub(crate) kind: PendingMutationKind,
    pub(crate) session_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) suspension_id: Option<String>,
    pub(crate) expected_session_version: Option<u64>,
    pub(crate) requested_through_position: Option<u64>,
    pub(crate) request_payload: Value,
    pub(crate) created_at: String,
}

impl std::fmt::Debug for PendingMutationDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingMutationDraft")
            .field("command_id", &self.command_id)
            .field("kind", &self.kind)
            .field("session_id", &self.session_id)
            .field("turn_id", &self.turn_id)
            .field("suspension_id", &self.suspension_id)
            .field("expected_session_version", &self.expected_session_version)
            .field(
                "requested_through_position",
                &self.requested_through_position,
            )
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedPendingIdentity {
    pub(crate) command_id: String,
    pub(crate) request_digest: String,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum HostReadResponse {
    Definitions(Vec<AgentDefinitionSummary>),
    SessionPage {
        request: SessionPageRequest,
        sessions: Vec<SessionSummary>,
        next_before: Option<String>,
    },
    Snapshot(Box<SnapshotRead>),
}

impl std::fmt::Debug for HostReadResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definitions(definitions) => formatter
                .debug_struct("Definitions")
                .field("count", &definitions.len())
                .finish(),
            Self::SessionPage {
                request,
                sessions,
                next_before,
            } => formatter
                .debug_struct("SessionPage")
                .field("request", request)
                .field("count", &sessions.len())
                .field("has_next", &next_before.is_some())
                .finish(),
            Self::Snapshot(snapshot) => formatter
                .debug_struct("Snapshot")
                .field("request", &snapshot.request)
                .field("count", &snapshot.items.len())
                .field("follow_position", &snapshot.follow_position)
                .finish(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HostReadFailure {
    pub(crate) code: HostClientErrorCode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectTag {
    Exit,
    LoadDefinitions,
    LoadSessionPage,
    LoadSnapshot,
    PersistPending,
    StartTurn,
    CreateSession,
    CancelTurn,
    ContinueTurn,
    PersistContinuation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppEffect {
    pub(crate) context: EffectContext,
    pub(crate) kind: EffectKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum EffectFailure {
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PersistenceFailure {
    Unavailable,
    UnsafePermissions,
    InvalidData,
    Conflict,
}

#[derive(Clone, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum AppEffectOutcome {
    Completed,
    Failed(EffectFailure),
    HostRead(Result<HostReadResponse, HostReadFailure>),
    PendingPersisted(Result<PersistedPendingIdentity, PersistenceFailure>),
}

impl std::fmt::Debug for AppEffectOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Completed => formatter.write_str("Completed"),
            Self::Failed(failure) => formatter.debug_tuple("Failed").field(failure).finish(),
            Self::HostRead(result) => formatter.debug_tuple("HostRead").field(result).finish(),
            Self::PendingPersisted(result) => formatter
                .debug_tuple("PendingPersisted")
                .field(result)
                .finish(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AppEffectResult {
    pub(crate) context: EffectContext,
    pub(crate) kind: EffectTag,
    pub(crate) outcome: AppEffectOutcome,
}

#[derive(Debug, Default)]
pub(crate) struct EffectTracker {
    pub(crate) generation: AppGeneration,
    next_effect_id: u64,
    pub(crate) pending: BTreeMap<EffectId, AppEffect>,
}

impl EffectTracker {
    pub(crate) fn issue(
        &mut self,
        kind: EffectKind,
        session_id: Option<String>,
        request_digest: Option<String>,
    ) -> Option<AppEffect> {
        let (effect_id, next) = EffectId::next(self.next_effect_id)?;
        self.next_effect_id = next;
        let effect = AppEffect {
            context: EffectContext {
                effect_id,
                issued_generation: self.generation,
                session_id,
                request_digest,
            },
            kind,
        };
        self.pending.insert(effect_id, effect.clone());
        Some(effect)
    }

    pub(crate) fn take_finished(&mut self, result: &AppEffectResult) -> Option<AppEffect> {
        let effect = self.pending.get(&result.context.effect_id)?;
        if effect.context != result.context || effect.kind.tag() != result.kind {
            return None;
        }
        self.pending.remove(&result.context.effect_id)
    }

    #[allow(dead_code)]
    pub(crate) fn issue_successor(
        &mut self,
        completed: &AppEffect,
        kind: EffectKind,
        request_digest: Option<String>,
    ) -> Option<AppEffect> {
        self.issue(kind, completed.context.session_id.clone(), request_digest)
    }

    pub(crate) fn has_pending_mutation(&self) -> bool {
        self.pending.values().any(is_pending_mutation)
    }

    pub(crate) fn has_pending_mutation_for_context(&self, session_id: Option<&str>) -> bool {
        self.pending.values().any(|effect| {
            is_pending_mutation(effect) && effect.context.session_id.as_deref() == session_id
        })
    }
}

fn is_pending_mutation(effect: &AppEffect) -> bool {
    matches!(
        effect.kind,
        EffectKind::PersistPending { .. } | EffectKind::PersistContinuation { .. }
    )
}
