use std::collections::BTreeMap;

use crate::{ExecutionId, FactDraft, LedgerError, ModelRequestId, ToolInvocationId, TurnId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TurnState {
    Open,
    Suspended,
    Completed,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutionState {
    Active,
    Completed,
    Suspended,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvocationState {
    Prepared,
    Authorized,
    Started,
    Receipt,
    Completed,
    Rejected,
    Interrupted,
    Unavailable,
    Failed,
    Denied,
    Uncertain,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SessionProjection {
    opened: bool,
    closed: bool,
    turns: BTreeMap<TurnId, TurnState>,
    executions: BTreeMap<ExecutionId, (TurnId, ExecutionState)>,
    models: BTreeMap<ModelRequestId, (ExecutionId, InvocationState)>,
    tools: BTreeMap<ToolInvocationId, (ExecutionId, InvocationState)>,
}

impl SessionProjection {
    pub(crate) fn apply(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        fact.validate()?;
        let kind = fact.kind.as_str();
        if kind == "session.opened" {
            if self.opened || self.closed {
                return Err(LedgerError::InvalidTransition);
            }
            self.opened = true;
            return Ok(());
        }
        if !self.opened || self.closed {
            return Err(LedgerError::InvalidTransition);
        }
        match kind {
            "session.closed" => self.close_session(),
            "turn.started" => self.start_turn(required(&fact.turn_id)?),
            "turn.suspended" => {
                self.transition_turn(required(&fact.turn_id)?, TurnState::Suspended)
            }
            "turn.completed" => {
                self.transition_turn(required(&fact.turn_id)?, TurnState::Completed)
            }
            "turn.stopped" => self.transition_turn(required(&fact.turn_id)?, TurnState::Stopped),
            "turn.failed" => self.transition_turn(required(&fact.turn_id)?, TurnState::Failed),
            "turn.input" => self.require_open_turn(required(&fact.turn_id)?),
            "execution.started" => self.start_execution(fact),
            "execution.completed" => self.transition_execution(fact, ExecutionState::Completed),
            "execution.suspended" => self.transition_execution(fact, ExecutionState::Suspended),
            "execution.stopped" => self.transition_execution(fact, ExecutionState::Stopped),
            "execution.failed" => self.transition_execution(fact, ExecutionState::Failed),
            "model.prepared" => self.prepare_model(fact),
            "model.started" => self.transition_model(fact, InvocationState::Started),
            "model.completed" => self.transition_model(fact, InvocationState::Completed),
            "model.rejected" => self.transition_model(fact, InvocationState::Rejected),
            "model.interrupted" => self.transition_model(fact, InvocationState::Interrupted),
            "model.unavailable" => self.transition_model(fact, InvocationState::Unavailable),
            "model.uncertain" => self.transition_model(fact, InvocationState::Uncertain),
            "effect.prepared" => self.prepare_tool(fact),
            "effect.authorized" => self.transition_tool(fact, InvocationState::Authorized),
            "effect.started" => self.transition_tool(fact, InvocationState::Started),
            "effect.receipt" => self.transition_tool(fact, InvocationState::Receipt),
            "effect.completed" => self.transition_tool(fact, InvocationState::Completed),
            "effect.failed" => self.transition_tool(fact, InvocationState::Failed),
            "effect.denied" => self.transition_tool(fact, InvocationState::Denied),
            "effect.uncertain" => self.transition_tool(fact, InvocationState::Uncertain),
            "interaction.requested"
            | "interaction.resolved"
            | "interaction.cancelled"
            | "context.summary"
            | "privacy.redacted" => Ok(()),
            _ => Ok(()),
        }
    }

    pub(crate) fn uncertain_model_requests(&self) -> Vec<ModelRequestId> {
        self.models
            .iter()
            .filter_map(|(identity, (_, state))| {
                (*state == InvocationState::Started).then_some(identity.clone())
            })
            .collect()
    }

    pub(crate) fn uncertain_tool_invocations(&self) -> Vec<ToolInvocationId> {
        self.tools
            .iter()
            .filter_map(|(identity, (_, state))| {
                (*state == InvocationState::Started).then_some(identity.clone())
            })
            .collect()
    }

    fn close_session(&mut self) -> Result<(), LedgerError> {
        if self
            .turns
            .values()
            .any(|state| matches!(state, TurnState::Open | TurnState::Suspended))
            || self
                .executions
                .values()
                .any(|(_, state)| *state == ExecutionState::Active)
            || self.has_recovery_pending_invocation(None)
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.closed = true;
        Ok(())
    }

    fn start_turn(&mut self, turn_id: &TurnId) -> Result<(), LedgerError> {
        match self.turns.get(turn_id) {
            None | Some(TurnState::Suspended) => {
                self.turns.insert(turn_id.clone(), TurnState::Open);
                Ok(())
            }
            _ => Err(LedgerError::InvalidTransition),
        }
    }

    fn require_open_turn(&self, turn_id: &TurnId) -> Result<(), LedgerError> {
        if self.turns.get(turn_id) == Some(&TurnState::Open) {
            Ok(())
        } else {
            Err(LedgerError::InvalidTransition)
        }
    }

    fn transition_turn(&mut self, turn_id: &TurnId, next: TurnState) -> Result<(), LedgerError> {
        let current = self
            .turns
            .get(turn_id)
            .ok_or(LedgerError::MissingReference)?;
        let valid = match next {
            TurnState::Suspended | TurnState::Completed => *current == TurnState::Open,
            TurnState::Stopped | TurnState::Failed => {
                matches!(current, TurnState::Open | TurnState::Suspended)
            }
            TurnState::Open => false,
        };
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        if self
            .executions
            .values()
            .any(|(owner, state)| owner == turn_id && *state == ExecutionState::Active)
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.turns.insert(turn_id.clone(), next);
        Ok(())
    }

    fn start_execution(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        self.require_open_turn(turn)?;
        let execution = required(&fact.execution_id)?;
        if self.executions.contains_key(execution) {
            return Err(LedgerError::InvalidTransition);
        }
        self.executions
            .insert(execution.clone(), (turn.clone(), ExecutionState::Active));
        Ok(())
    }

    fn transition_execution(
        &mut self,
        fact: &FactDraft,
        next: ExecutionState,
    ) -> Result<(), LedgerError> {
        let execution = required(&fact.execution_id)?;
        let turn = required(&fact.turn_id)?;
        let (owned_turn, state) = self
            .executions
            .get(execution)
            .ok_or(LedgerError::MissingReference)?;
        if owned_turn != turn || *state != ExecutionState::Active {
            return Err(LedgerError::InvalidTransition);
        }
        if self.has_recovery_pending_invocation(Some(execution)) {
            return Err(LedgerError::InvalidTransition);
        }
        let (_, state) = self
            .executions
            .get_mut(execution)
            .expect("validated execution remains present");
        *state = next;
        Ok(())
    }

    fn prepare_model(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let request = required(&fact.model_request_id)?;
        let execution = required(&fact.execution_id)?;
        if self
            .models
            .insert(
                request.clone(),
                (execution.clone(), InvocationState::Prepared),
            )
            .is_some()
        {
            return Err(LedgerError::InvalidTransition);
        }
        Ok(())
    }

    fn transition_model(
        &mut self,
        fact: &FactDraft,
        next: InvocationState,
    ) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let request = required(&fact.model_request_id)?;
        let execution = required(&fact.execution_id)?;
        let (owner, state) = self
            .models
            .get_mut(request)
            .ok_or(LedgerError::MissingReference)?;
        if owner != execution {
            return Err(LedgerError::InvalidTransition);
        }
        let valid = (*state == InvocationState::Prepared && next == InvocationState::Started)
            || (*state == InvocationState::Started
                && !matches!(next, InvocationState::Prepared | InvocationState::Started));
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        *state = next;
        Ok(())
    }

    fn prepare_tool(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let tool = required(&fact.tool_invocation_id)?;
        let execution = required(&fact.execution_id)?;
        if self
            .tools
            .insert(tool.clone(), (execution.clone(), InvocationState::Prepared))
            .is_some()
        {
            return Err(LedgerError::InvalidTransition);
        }
        Ok(())
    }

    fn transition_tool(
        &mut self,
        fact: &FactDraft,
        next: InvocationState,
    ) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let tool = required(&fact.tool_invocation_id)?;
        let execution = required(&fact.execution_id)?;
        let (owner, state) = self
            .tools
            .get_mut(tool)
            .ok_or(LedgerError::MissingReference)?;
        if owner != execution {
            return Err(LedgerError::InvalidTransition);
        }
        let valid = matches!(
            (*state, next),
            (InvocationState::Prepared, InvocationState::Authorized)
                | (InvocationState::Prepared, InvocationState::Started)
                | (InvocationState::Prepared, InvocationState::Denied)
                | (InvocationState::Authorized, InvocationState::Started)
                | (InvocationState::Authorized, InvocationState::Denied)
                | (InvocationState::Started, InvocationState::Receipt)
                | (InvocationState::Started, InvocationState::Completed)
                | (InvocationState::Started, InvocationState::Failed)
                | (InvocationState::Started, InvocationState::Uncertain)
                | (InvocationState::Receipt, InvocationState::Completed)
                | (InvocationState::Receipt, InvocationState::Failed)
        );
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        *state = next;
        Ok(())
    }

    fn require_active_execution(&self, fact: &FactDraft) -> Result<(), LedgerError> {
        let execution = required(&fact.execution_id)?;
        let turn = required(&fact.turn_id)?;
        match self.executions.get(execution) {
            Some((owned_turn, ExecutionState::Active)) if owned_turn == turn => Ok(()),
            Some(_) => Err(LedgerError::InvalidTransition),
            None => Err(LedgerError::MissingReference),
        }
    }

    fn has_recovery_pending_invocation(&self, execution: Option<&ExecutionId>) -> bool {
        let pending = |owner: &ExecutionId, state: InvocationState| {
            execution.map_or(true, |expected| owner == expected)
                && matches!(state, InvocationState::Started | InvocationState::Receipt)
        };
        self.models
            .values()
            .any(|(owner, state)| pending(owner, *state))
            || self
                .tools
                .values()
                .any(|(owner, state)| pending(owner, *state))
    }
}

fn required<T>(value: &Option<T>) -> Result<&T, LedgerError> {
    value.as_ref().ok_or(LedgerError::MissingReference)
}
