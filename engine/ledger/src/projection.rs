use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

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
    Abandoned,
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
    Reconciled,
    Observed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InteractionRecord {
    execution: ExecutionId,
    tool: ToolInvocationId,
    suspension_id: String,
    prepared_digest: String,
    terminal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KnowledgeRecord {
    execution: ExecutionId,
    request_digest: String,
    dispatch_attempts: BTreeSet<String>,
    terminal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScheduleState {
    Active,
    Superseding,
    Cancelled,
    Failed,
    Exhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScheduleClaim {
    occurrence_id: String,
    ordinal: u64,
    due_at_utc: String,
    lease_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScheduleRecord {
    revision_id: String,
    intent_digest: String,
    last_handled_ordinal: u64,
    pending_claim: Option<ScheduleClaim>,
    state: ScheduleState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SessionProjection {
    opened: bool,
    closed: bool,
    turns: BTreeMap<TurnId, TurnState>,
    executions: BTreeMap<ExecutionId, (TurnId, ExecutionState)>,
    execution_iterations: BTreeMap<ExecutionId, u64>,
    models: BTreeMap<ModelRequestId, (ExecutionId, InvocationState)>,
    model_digests: BTreeMap<ModelRequestId, String>,
    tools: BTreeMap<ToolInvocationId, (ExecutionId, InvocationState)>,
    tool_digests: BTreeMap<ToolInvocationId, String>,
    tool_grants: BTreeMap<ToolInvocationId, String>,
    tool_receipts: BTreeMap<ToolInvocationId, String>,
    tool_executors: BTreeMap<ToolInvocationId, (String, String)>,
    tool_reconciled_observations: BTreeMap<ToolInvocationId, Value>,
    suspensions: BTreeMap<TurnId, String>,
    interactions: BTreeMap<String, InteractionRecord>,
    knowledge: BTreeMap<String, KnowledgeRecord>,
    schedules: BTreeMap<String, ScheduleRecord>,
}

impl SessionProjection {
    pub(crate) fn recoverable_turns(&self) -> Vec<TurnId> {
        self.turns
            .iter()
            .filter_map(|(turn, state)| {
                matches!(state, TurnState::Open | TurnState::Suspended).then_some(turn.clone())
            })
            .collect()
    }

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
            "turn.started" => self.start_turn(fact),
            "turn.suspended" => self.suspend_turn(fact),
            "turn.completed" => self.terminal_turn(fact, TurnState::Completed),
            "turn.stopped" => self.terminal_turn(fact, TurnState::Stopped),
            "turn.failed" => self.terminal_turn(fact, TurnState::Failed),
            "turn.input" => self.admit_turn_input(fact),
            "turn.cancel_requested" => self.require_non_terminal_turn(required(&fact.turn_id)?),
            "execution.started" => self.start_execution(fact),
            "execution.iteration_started" => self.start_iteration(fact),
            "execution.abandoned" => self.transition_execution(fact, ExecutionState::Abandoned),
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
            "effect.reconciled" => self.reconcile_tool(fact),
            "effect.observation" => self.observe_tool(fact),
            "tool.preparation_rejected" => self.reject_tool_preparation(fact),
            "interaction.requested" => self.request_interaction(fact),
            "interaction.resolved" | "interaction.cancelled" => self.finish_interaction(fact),
            "knowledge.requested" => self.request_knowledge(fact),
            "knowledge.dispatched" => self.dispatch_knowledge(fact),
            "knowledge.completed" | "knowledge.failed" => self.terminal_knowledge(fact),
            "schedule.created" => self.create_schedule(fact),
            "schedule.claimed" => self.claim_schedule(fact),
            "schedule.fired" => self.fire_schedule(fact),
            "schedule.skipped" => self.skip_schedule(fact),
            "schedule.cancelled" => self.cancel_schedule(fact),
            "schedule.failed" => self.fail_schedule(fact),
            "schedule.exhausted" => self.exhaust_schedule(fact),
            "context.summary" | "privacy.redacted" => Ok(()),
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
            || self
                .schedules
                .values()
                .any(|value| value.state == ScheduleState::Active)
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.closed = true;
        Ok(())
    }

    fn start_turn(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        let payload = payload(fact)?;
        let kind = text(&payload, "kind")?;
        let prior = payload.get("prior_suspension_id").and_then(Value::as_str);
        let valid = match self.turns.get(turn) {
            None => kind == "start" && prior.is_none(),
            Some(TurnState::Suspended) => {
                kind == "continue"
                    && self.suspensions.get(turn).map(String::as_str) == prior
                    && !self.has_pending_interaction_for_turn(turn)
            }
            _ => false,
        };
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        self.turns.insert(turn.clone(), TurnState::Open);
        self.suspensions.remove(turn);
        Ok(())
    }

    fn suspend_turn(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        let payload = payload(fact)?;
        let execution = ExecutionId::try_from(text(&payload, "execution_id")?)
            .map_err(|_| LedgerError::InvalidFact)?;
        if self.executions.get(&execution) != Some(&(turn.clone(), ExecutionState::Suspended)) {
            return Err(LedgerError::InvalidTransition);
        }
        self.transition_turn(turn, TurnState::Suspended)?;
        self.suspensions
            .insert(turn.clone(), text(&payload, "suspension_id")?.to_owned());
        Ok(())
    }

    fn terminal_turn(&mut self, fact: &FactDraft, next: TurnState) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        let payload = payload(fact)?;
        let execution = ExecutionId::try_from(text(&payload, "execution_id")?)
            .map_err(|_| LedgerError::InvalidFact)?;
        let expected = match next {
            TurnState::Completed => ExecutionState::Completed,
            TurnState::Stopped => ExecutionState::Stopped,
            TurnState::Failed => ExecutionState::Failed,
            _ => return Err(LedgerError::InvalidTransition),
        };
        let actual = self.executions.get(&execution);
        let suspended_close = matches!(next, TurnState::Stopped | TurnState::Failed)
            && self.turns.get(turn) == Some(&TurnState::Suspended)
            && actual == Some(&(turn.clone(), ExecutionState::Suspended));
        if actual != Some(&(turn.clone(), expected)) && !suspended_close {
            return Err(LedgerError::InvalidTransition);
        }
        self.transition_turn(turn, next)
    }

    fn require_open_turn(&self, turn_id: &TurnId) -> Result<(), LedgerError> {
        if self.turns.get(turn_id) == Some(&TurnState::Open) {
            Ok(())
        } else {
            Err(LedgerError::InvalidTransition)
        }
    }

    fn admit_turn_input(&self, fact: &FactDraft) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        let payload = payload(fact)?;
        if !matches!(
            text(&payload, "input_kind")?,
            "trusted_user" | "trusted_system"
        ) {
            if self.turns.get(turn) == Some(&TurnState::Suspended)
                && self.suspensions.get(turn).map(String::as_str)
                    == payload.get("suspension_id").and_then(Value::as_str)
            {
                Ok(())
            } else {
                Err(LedgerError::InvalidTransition)
            }
        } else {
            self.require_open_turn(turn)
        }
    }

    fn require_non_terminal_turn(&self, turn_id: &TurnId) -> Result<(), LedgerError> {
        match self.turns.get(turn_id) {
            Some(TurnState::Open | TurnState::Suspended) => Ok(()),
            Some(_) => Err(LedgerError::InvalidTransition),
            None => Err(LedgerError::MissingReference),
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
        if next != TurnState::Suspended && self.has_pending_interaction_for_turn(turn_id) {
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
        self.execution_iterations.insert(
            execution.clone(),
            unsigned(&payload(fact)?, "completed_iterations")?,
        );
        Ok(())
    }

    fn start_iteration(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let execution = required(&fact.execution_id)?;
        let iteration = unsigned(&payload(fact)?, "iteration")?;
        let current = self
            .execution_iterations
            .get_mut(execution)
            .ok_or(LedgerError::MissingReference)?;
        if current.checked_add(1) != Some(iteration) {
            return Err(LedgerError::InvalidTransition);
        }
        *current = iteration;
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
        if self.has_recovery_pending_invocation(Some(execution))
            || self.has_pending_knowledge(execution)
            || (next != ExecutionState::Suspended && self.has_pending_interaction(execution))
        {
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
        self.model_digests.insert(
            request.clone(),
            text(&payload(fact)?, "request_digest")?.to_owned(),
        );
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
        if self.model_digests.get(request).map(String::as_str)
            != Some(text(&payload(fact)?, "request_digest")?)
        {
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
        self.tool_digests.insert(
            tool.clone(),
            text(&payload(fact)?, "prepared_digest")?.to_owned(),
        );
        Ok(())
    }

    fn request_interaction(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let tool = required(&fact.tool_invocation_id)?;
        let execution = required(&fact.execution_id)?;
        if !matches!(
            self.tools.get(tool),
            Some((owner, InvocationState::Prepared | InvocationState::Authorized)) if owner == execution
        ) {
            return Err(LedgerError::InvalidTransition);
        }
        let payload = payload(fact)?;
        let interaction_id = text(&payload, "interaction_id")?.to_owned();
        let prepared_digest = text(&payload, "prepared_digest")?.to_owned();
        if self.tool_digests.get(tool) != Some(&prepared_digest)
            || self.interactions.contains_key(&interaction_id)
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.interactions.insert(
            interaction_id,
            InteractionRecord {
                execution: execution.clone(),
                tool: tool.clone(),
                suspension_id: text(&payload, "suspension_id")?.to_owned(),
                prepared_digest,
                terminal: false,
            },
        );
        Ok(())
    }

    fn finish_interaction(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let execution = required(&fact.execution_id)?;
        let tool = required(&fact.tool_invocation_id)?;
        let payload = payload(fact)?;
        let interaction = self
            .interactions
            .get_mut(text(&payload, "interaction_id")?)
            .ok_or(LedgerError::MissingReference)?;
        if interaction.terminal
            || &interaction.execution != execution
            || &interaction.tool != tool
            || interaction.suspension_id != text(&payload, "suspension_id")?
            || interaction.prepared_digest != text(&payload, "prepared_digest")?
        {
            return Err(LedgerError::InvalidTransition);
        }
        interaction.terminal = true;
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
        let payload = payload(fact)?;
        if self.tool_digests.get(tool).map(String::as_str)
            != Some(text(&payload, "prepared_digest")?)
        {
            return Err(LedgerError::InvalidTransition);
        }
        let (owner, state) = self.tools.get(tool).ok_or(LedgerError::MissingReference)?;
        if owner != execution {
            return Err(LedgerError::InvalidTransition);
        }
        let current = *state;
        let valid = matches!(
            (current, next),
            (InvocationState::Prepared, InvocationState::Authorized)
                | (InvocationState::Prepared, InvocationState::Started)
                | (InvocationState::Prepared, InvocationState::Denied)
                | (InvocationState::Authorized, InvocationState::Started)
                | (InvocationState::Authorized, InvocationState::Denied)
                | (InvocationState::Authorized, InvocationState::Failed)
                | (InvocationState::Started, InvocationState::Receipt)
                | (InvocationState::Started, InvocationState::Failed)
                | (InvocationState::Started, InvocationState::Uncertain)
                | (InvocationState::Receipt, InvocationState::Completed)
                | (InvocationState::Receipt, InvocationState::Failed)
        );
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        self.validate_effect_binding(tool, current, next, &payload)?;
        self.tools
            .get_mut(tool)
            .expect("validated tool remains present")
            .1 = next;
        Ok(())
    }

    fn validate_effect_binding(
        &mut self,
        tool: &ToolInvocationId,
        current: InvocationState,
        next: InvocationState,
        payload: &Map<String, Value>,
    ) -> Result<(), LedgerError> {
        match next {
            InvocationState::Authorized => {
                self.tool_grants
                    .insert(tool.clone(), text(payload, "grant_id")?.to_owned());
            }
            InvocationState::Started => {
                let grant = text(payload, "grant_id")?;
                if self
                    .tool_grants
                    .get(tool)
                    .is_some_and(|value| value != grant)
                {
                    return Err(LedgerError::InvalidTransition);
                }
                self.tool_grants.insert(tool.clone(), grant.to_owned());
                self.tool_executors.insert(
                    tool.clone(),
                    (
                        text(payload, "executor_id")?.to_owned(),
                        text(payload, "executor_revision")?.to_owned(),
                    ),
                );
            }
            InvocationState::Receipt => {
                if self.tool_grants.get(tool).map(String::as_str)
                    != Some(text(payload, "grant_id")?)
                    || self.tool_executors.get(tool)
                        != Some(&(
                            text(payload, "executor_id")?.to_owned(),
                            text(payload, "executor_revision")?.to_owned(),
                        ))
                {
                    return Err(LedgerError::InvalidTransition);
                }
                self.tool_receipts
                    .insert(tool.clone(), text(payload, "receipt_id")?.to_owned());
            }
            InvocationState::Completed => {
                if self.tool_receipts.get(tool).map(String::as_str)
                    != Some(text(payload, "receipt_id")?)
                {
                    return Err(LedgerError::InvalidTransition);
                }
            }
            InvocationState::Failed
                if current == InvocationState::Receipt
                    && self.tool_receipts.get(tool).map(String::as_str)
                        != payload.get("receipt_id").and_then(Value::as_str) =>
            {
                return Err(LedgerError::InvalidTransition);
            }
            _ => {}
        }
        Ok(())
    }

    fn observe_tool(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let tool = required(&fact.tool_invocation_id)?;
        let execution = required(&fact.execution_id)?;
        let payload = payload(fact)?;
        let (owner, state) = self.tools.get(tool).ok_or(LedgerError::MissingReference)?;
        let state = *state;
        if owner != execution
            || !matches!(
                state,
                InvocationState::Completed
                    | InvocationState::Failed
                    | InvocationState::Denied
                    | InvocationState::Reconciled
            )
        {
            return Err(LedgerError::InvalidTransition);
        }
        if state == InvocationState::Reconciled {
            self.require_suspended_execution(fact)?;
            if self.tool_reconciled_observations.get(tool) != payload.get("observation") {
                return Err(LedgerError::InvalidTransition);
            }
        } else {
            self.require_active_execution(fact)?;
        }
        if self.tool_digests.get(tool).map(String::as_str)
            != Some(text(&payload, "prepared_digest")?)
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.tools
            .get_mut(tool)
            .expect("validated tool remains present")
            .1 = InvocationState::Observed;
        Ok(())
    }

    fn reconcile_tool(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_suspended_execution(fact)?;
        let tool = required(&fact.tool_invocation_id)?;
        let execution = required(&fact.execution_id)?;
        let payload = payload(fact)?;
        if self.tool_digests.get(tool).map(String::as_str)
            != Some(text(&payload, "prepared_digest")?)
            || self.tools.get(tool) != Some(&(execution.clone(), InvocationState::Uncertain))
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.tools
            .get_mut(tool)
            .expect("validated tool remains present")
            .1 = InvocationState::Reconciled;
        self.tool_reconciled_observations.insert(
            tool.clone(),
            payload
                .get("observation")
                .cloned()
                .ok_or(LedgerError::InvalidFact)?,
        );
        Ok(())
    }

    fn reject_tool_preparation(&self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        if fact.tool_invocation_id.is_some() {
            return Err(LedgerError::InvalidTransition);
        }
        let request = required(&fact.model_request_id)?;
        let execution = required(&fact.execution_id)?;
        if text(&payload(fact)?, "source_model_request_id")? != request.as_str() {
            return Err(LedgerError::InvalidTransition);
        }
        match self.models.get(request) {
            Some((owner, InvocationState::Completed)) if owner == execution => Ok(()),
            Some(_) => Err(LedgerError::InvalidTransition),
            None => Err(LedgerError::MissingReference),
        }
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

    fn require_suspended_execution(&self, fact: &FactDraft) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        let execution = required(&fact.execution_id)?;
        if self.turns.get(turn) == Some(&TurnState::Suspended)
            && self.executions.get(execution) == Some(&(turn.clone(), ExecutionState::Suspended))
        {
            Ok(())
        } else {
            Err(LedgerError::InvalidTransition)
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

    fn request_knowledge(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let execution = required(&fact.execution_id)?.clone();
        let value = payload(fact)?;
        let request_id = text(&value, "request_id")?.to_owned();
        let record = KnowledgeRecord {
            execution,
            request_digest: text(&value, "request_digest")?.to_owned(),
            dispatch_attempts: BTreeSet::new(),
            terminal: false,
        };
        if self.knowledge.insert(request_id, record).is_some() {
            Err(LedgerError::InvalidTransition)
        } else {
            Ok(())
        }
    }

    fn dispatch_knowledge(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let execution = required(&fact.execution_id)?;
        let value = payload(fact)?;
        let record = self
            .knowledge
            .get_mut(text(&value, "request_id")?)
            .ok_or(LedgerError::InvalidTransition)?;
        let attempt = text(&value, "dispatch_attempt_id")?.to_owned();
        if &record.execution != execution
            || record.terminal
            || record.request_digest != text(&value, "request_digest")?
            || !record.dispatch_attempts.insert(attempt)
        {
            Err(LedgerError::InvalidTransition)
        } else {
            Ok(())
        }
    }

    fn terminal_knowledge(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let execution = required(&fact.execution_id)?;
        let value = payload(fact)?;
        let record = self
            .knowledge
            .get_mut(text(&value, "request_id")?)
            .ok_or(LedgerError::InvalidTransition)?;
        let dispatched = !record.dispatch_attempts.is_empty();
        let valid_phase = if fact.kind.as_str() == "knowledge.completed" {
            dispatched
        } else {
            match text(&value, "phase")? {
                "pre_dispatch" => !dispatched,
                "dispatched" | "response_validation" => dispatched,
                _ => false,
            }
        };
        if &record.execution != execution
            || record.terminal
            || record.request_digest != text(&value, "request_digest")?
            || !valid_phase
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.terminal = true;
        Ok(())
    }

    fn has_pending_knowledge(&self, execution: &ExecutionId) -> bool {
        self.knowledge
            .values()
            .any(|value| &value.execution == execution && !value.terminal)
    }

    pub(crate) fn validate_commit_boundary(&self) -> Result<(), LedgerError> {
        if self
            .schedules
            .values()
            .any(|value| value.state == ScheduleState::Superseding)
        {
            Err(LedgerError::InvalidTransition)
        } else {
            Ok(())
        }
    }

    fn create_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let schedule_id = text(&value, "schedule_id")?.to_owned();
        let revision_id = text(&value, "revision_id")?.to_owned();
        let intent_digest = text(&value, "intent_digest")?.to_owned();
        if let Some(existing) = self.schedules.get_mut(&schedule_id) {
            if existing.state != ScheduleState::Superseding || existing.revision_id == revision_id {
                return Err(LedgerError::InvalidTransition);
            }
            *existing = ScheduleRecord {
                revision_id,
                intent_digest,
                last_handled_ordinal: 0,
                pending_claim: None,
                state: ScheduleState::Active,
            };
        } else {
            self.schedules.insert(
                schedule_id,
                ScheduleRecord {
                    revision_id,
                    intent_digest,
                    last_handled_ordinal: 0,
                    pending_claim: None,
                    state: ScheduleState::Active,
                },
            );
        }
        Ok(())
    }

    fn claim_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let record = self.schedule_mut(&value)?;
        let ordinal = unsigned(&value, "ordinal")?;
        let occurrence_id = text(&value, "occurrence_id")?.to_owned();
        let due_at_utc = text(&value, "due_at_utc")?.to_owned();
        let lease_epoch = unsigned(&value, "lease_epoch")?;
        let next = record
            .last_handled_ordinal
            .checked_add(1)
            .ok_or(LedgerError::InvalidTransition)?;
        let valid = match &record.pending_claim {
            None => ordinal == next,
            Some(pending) => {
                pending.ordinal == ordinal
                    && pending.occurrence_id == occurrence_id
                    && pending.due_at_utc == due_at_utc
                    && lease_epoch > pending.lease_epoch
            }
        };
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        record.pending_claim = Some(ScheduleClaim {
            occurrence_id,
            ordinal,
            due_at_utc,
            lease_epoch,
        });
        Ok(())
    }

    fn fire_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let record = self.schedule_mut(&value)?;
        let ordinal = unsigned(&value, "ordinal")?;
        let occurrence = text(&value, "occurrence_id")?;
        if !record.pending_claim.as_ref().is_some_and(|pending| {
            pending.ordinal == ordinal && pending.occurrence_id == occurrence
        }) {
            return Err(LedgerError::InvalidTransition);
        }
        record.last_handled_ordinal = ordinal;
        record.pending_claim = None;
        Ok(())
    }

    fn skip_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let record = self.schedule_mut(&value)?;
        let first = unsigned(&value, "first_ordinal")?;
        let last = unsigned(&value, "last_ordinal")?;
        if record.pending_claim.is_some()
            || record.last_handled_ordinal.checked_add(1) != Some(first)
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.last_handled_ordinal = last;
        Ok(())
    }

    fn cancel_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let schedule_id = text(&value, "schedule_id")?;
        let record = self
            .schedules
            .get_mut(schedule_id)
            .ok_or(LedgerError::InvalidTransition)?;
        if record.state != ScheduleState::Active
            || record.pending_claim.is_some()
            || record.revision_id != text(&value, "expected_revision_id")?
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.state = if text(&value, "reason")? == "superseded" {
            ScheduleState::Superseding
        } else {
            ScheduleState::Cancelled
        };
        Ok(())
    }

    fn fail_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let record = self.schedule_mut(&value)?;
        let occurrence = value.get("occurrence_id").and_then(Value::as_str);
        let ordinal = value.get("ordinal").and_then(Value::as_u64);
        let matches_claim = match (&record.pending_claim, occurrence, ordinal) {
            (None, None, None) => true,
            (None, Some(_), Some(number)) => {
                record.last_handled_ordinal.checked_add(1) == Some(number)
            }
            (Some(pending), Some(id), Some(number)) => {
                pending.occurrence_id == id && pending.ordinal == number
            }
            _ => false,
        };
        if !matches_claim {
            return Err(LedgerError::InvalidTransition);
        }
        record.pending_claim = None;
        record.state = ScheduleState::Failed;
        Ok(())
    }

    fn exhaust_schedule(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let record = self.schedule_mut(&value)?;
        if record.pending_claim.is_some()
            || record.last_handled_ordinal != unsigned(&value, "last_handled_ordinal")?
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.state = ScheduleState::Exhausted;
        Ok(())
    }

    fn schedule_mut(
        &mut self,
        value: &Map<String, Value>,
    ) -> Result<&mut ScheduleRecord, LedgerError> {
        let record = self
            .schedules
            .get_mut(text(value, "schedule_id")?)
            .ok_or(LedgerError::InvalidTransition)?;
        if record.state != ScheduleState::Active
            || record.revision_id != text(value, "revision_id")?
        {
            Err(LedgerError::InvalidTransition)
        } else {
            Ok(record)
        }
    }

    fn has_pending_interaction(&self, execution: &ExecutionId) -> bool {
        self.interactions
            .values()
            .any(|value| &value.execution == execution && !value.terminal)
    }

    fn has_pending_interaction_for_turn(&self, turn: &TurnId) -> bool {
        self.interactions.values().any(|value| {
            !value.terminal
                && self
                    .executions
                    .get(&value.execution)
                    .is_some_and(|(owner, _)| owner == turn)
        })
    }
}

fn required<T>(value: &Option<T>) -> Result<&T, LedgerError> {
    value.as_ref().ok_or(LedgerError::MissingReference)
}

fn payload(fact: &FactDraft) -> Result<Map<String, Value>, LedgerError> {
    let value: Value =
        serde_json::from_str(fact.payload.as_json()).map_err(|_| LedgerError::InvalidFact)?;
    value.as_object().cloned().ok_or(LedgerError::InvalidFact)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, LedgerError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(LedgerError::InvalidFact)
}

fn unsigned(value: &Map<String, Value>, key: &str) -> Result<u64, LedgerError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(LedgerError::InvalidFact)
}
