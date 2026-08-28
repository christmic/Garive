use std::collections::{BTreeMap, BTreeSet};

use crate::projection::SessionProjection;
use crate::{
    CommitDisposition, CommitResult, DurableFact, FactDraft, FactId, FactKind, LedgerError,
    ModelRequestId, SessionId, ToolInvocationId, TurnId,
};

#[derive(Clone, Debug, Default)]
struct SessionLedger {
    version: u64,
    facts: Vec<DurableFact>,
    drafts: BTreeMap<u64, FactDraft>,
    projection: SessionProjection,
}

#[derive(Clone, Debug)]
struct FactIndexEntry {
    session_id: SessionId,
    position: u64,
    draft: FactDraft,
}

#[derive(Clone, Debug, Default)]
pub struct LedgerState {
    sessions: BTreeMap<SessionId, SessionLedger>,
    fact_index: BTreeMap<FactId, FactIndexEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnSnapshot {
    pub facts: Vec<DurableFact>,
    pub session_version: u64,
    pub through_position: u64,
}

impl LedgerState {
    pub fn commit(
        &mut self,
        session_id: SessionId,
        expected_session_version: u64,
        drafts: Vec<FactDraft>,
    ) -> Result<CommitResult, LedgerError> {
        if drafts.is_empty() {
            return Err(LedgerError::EmptyBatch);
        }
        let mut identities = BTreeSet::new();
        let mut replay_positions = Vec::with_capacity(drafts.len());
        let mut replayed = 0usize;
        for draft in &drafts {
            draft.validate()?;
            if !identities.insert(draft.fact_id.clone()) {
                return Err(LedgerError::InvalidFact);
            }
            if let Some(existing) = self.fact_index.get(&draft.fact_id) {
                if existing.session_id != session_id || !existing.draft.same_semantics(draft) {
                    return Err(LedgerError::IdempotencyCollision);
                }
                replayed += 1;
                replay_positions.push(existing.position);
            }
        }
        if replayed == drafts.len() {
            let version = self
                .sessions
                .get(&session_id)
                .ok_or(LedgerError::MissingReference)?
                .version;
            return Ok(CommitResult {
                disposition: CommitDisposition::Replayed,
                session_version: version,
                positions: replay_positions,
            });
        }
        if replayed != 0 {
            return Err(LedgerError::IncompleteReplay);
        }

        let mut next = self.sessions.get(&session_id).cloned().unwrap_or_default();
        if next.version != expected_session_version {
            return Err(LedgerError::ConcurrentModification);
        }
        let mut position = next.facts.last().map_or(Ok(1), |fact| {
            fact.position
                .checked_add(1)
                .ok_or(LedgerError::PositionOverflow)
        })?;
        let mut positions = Vec::with_capacity(drafts.len());
        let last_index = drafts.len() - 1;
        for (index, draft) in drafts.iter().cloned().enumerate() {
            next.projection.apply(&draft)?;
            let durable = DurableFact::from((session_id.clone(), position, draft.clone()));
            durable.verify()?;
            next.drafts.insert(position, draft);
            next.facts.push(durable);
            positions.push(position);
            if index != last_index {
                position = position
                    .checked_add(1)
                    .ok_or(LedgerError::PositionOverflow)?;
            }
        }
        next.version = next
            .version
            .checked_add(1)
            .ok_or(LedgerError::PositionOverflow)?;

        for (draft, position) in drafts.iter().cloned().zip(positions.iter().copied()) {
            self.fact_index.insert(
                draft.fact_id.clone(),
                FactIndexEntry {
                    session_id: session_id.clone(),
                    position,
                    draft,
                },
            );
        }
        let session_version = next.version;
        self.sessions.insert(session_id, next);
        Ok(CommitResult {
            disposition: CommitDisposition::Committed,
            session_version,
            positions,
        })
    }

    pub fn read_facts(
        &self,
        session_id: &SessionId,
        after_position: u64,
        through_position: u64,
        kinds: Option<&BTreeSet<FactKind>>,
    ) -> Result<Vec<DurableFact>, LedgerError> {
        if through_position == 0 || after_position >= through_position {
            return Err(LedgerError::InvalidReadRange);
        }
        let session = self
            .sessions
            .get(session_id)
            .ok_or(LedgerError::MissingReference)?;
        let mut previous = after_position;
        let mut output = Vec::new();
        for fact in session
            .facts
            .iter()
            .filter(|fact| fact.position > after_position && fact.position <= through_position)
        {
            fact.verify()?;
            if fact.position <= previous {
                return Err(LedgerError::InvalidTransition);
            }
            previous = fact.position;
            if kinds.map_or(true, |values| values.contains(&fact.kind)) {
                output.push(fact.clone());
            }
        }
        Ok(output)
    }

    pub fn load_turn(&self, turn_id: &TurnId) -> Result<TurnSnapshot, LedgerError> {
        for session in self.sessions.values() {
            let facts: Vec<_> = session
                .facts
                .iter()
                .filter(|fact| fact.turn_id.as_ref() == Some(turn_id))
                .cloned()
                .collect();
            if !facts.is_empty() {
                return Ok(TurnSnapshot {
                    through_position: session.facts.last().map_or(0, |fact| fact.position),
                    session_version: session.version,
                    facts,
                });
            }
        }
        Err(LedgerError::MissingReference)
    }

    pub fn find_model_request(&self, request_id: &ModelRequestId) -> Vec<DurableFact> {
        self.find_invocation(|fact| fact.model_request_id.as_ref() == Some(request_id))
    }

    pub fn find_tool_invocation(&self, invocation_id: &ToolInvocationId) -> Vec<DurableFact> {
        self.find_invocation(|fact| fact.tool_invocation_id.as_ref() == Some(invocation_id))
    }

    pub fn list_uncertain_model_requests(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ModelRequestId>, LedgerError> {
        self.sessions
            .get(session_id)
            .map(|session| session.projection.uncertain_model_requests())
            .ok_or(LedgerError::MissingReference)
    }

    pub fn list_uncertain_tool_invocations(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<ToolInvocationId>, LedgerError> {
        self.sessions
            .get(session_id)
            .map(|session| session.projection.uncertain_tool_invocations())
            .ok_or(LedgerError::MissingReference)
    }

    pub fn session_version(&self, session_id: &SessionId) -> Option<u64> {
        self.sessions.get(session_id).map(|session| session.version)
    }

    pub fn fact_count(&self, session_id: &SessionId) -> usize {
        self.sessions
            .get(session_id)
            .map_or(0, |session| session.facts.len())
    }

    pub fn fact_at(&self, session_id: &SessionId, position: u64) -> Option<DurableFact> {
        self.sessions
            .get(session_id)
            .and_then(|session| session.facts.iter().find(|fact| fact.position == position))
            .cloned()
    }

    fn find_invocation(&self, predicate: impl Fn(&DurableFact) -> bool) -> Vec<DurableFact> {
        self.sessions
            .values()
            .flat_map(|session| session.facts.iter())
            .filter(|fact| predicate(fact))
            .cloned()
            .collect()
    }
}
