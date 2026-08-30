use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::{
    CanonicalPayload, ExecutionId, FactDraft, LedgerError, ModelRequestId, ToolInvocationId, TurnId,
};

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

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectBatchRecord {
    tools: Vec<ToolInvocationId>,
    next_start: usize,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DelegationState {
    Requested,
    Authorized,
    Denied,
    Started,
    Terminal,
    Observed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DelegationRecord {
    parent_turn: TurnId,
    parent_execution: ExecutionId,
    intent_digest: String,
    grant_id: Option<String>,
    suspension_id: Option<String>,
    child_turn: Option<TurnId>,
    result_id: Option<String>,
    result_digest: Option<String>,
    input_admitted: bool,
    state: DelegationState,
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
    tool_contract_versions: BTreeMap<ToolInvocationId, u32>,
    tool_result_bytes: BTreeMap<ToolInvocationId, u64>,
    tool_batch_plans: BTreeMap<ToolInvocationId, String>,
    effect_batches: BTreeMap<String, EffectBatchRecord>,
    tool_grants: BTreeMap<ToolInvocationId, String>,
    tool_receipts: BTreeMap<ToolInvocationId, String>,
    tool_executors: BTreeMap<ToolInvocationId, (String, String)>,
    tool_reconciled_observations: BTreeMap<ToolInvocationId, Value>,
    suspensions: BTreeMap<TurnId, String>,
    interactions: BTreeMap<String, InteractionRecord>,
    knowledge: BTreeMap<String, KnowledgeRecord>,
    schedules: BTreeMap<String, ScheduleRecord>,
    turn_agents: BTreeMap<TurnId, (String, String)>,
    delegations: BTreeMap<String, DelegationRecord>,
    turns_started_in_commit: BTreeSet<TurnId>,
    turns_suspended_in_commit: BTreeSet<TurnId>,
    turns_terminal_in_commit: BTreeSet<TurnId>,
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
            "execution.effect_batch_planned" => self.plan_effect_batch(fact),
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
            "delegation.requested" => self.request_delegation(fact),
            "delegation.authorized" => self.authorize_delegation(fact),
            "delegation.denied" => self.deny_delegation(fact),
            "delegation.child_started" => self.start_delegation_child(fact),
            "delegation.child_terminal" => self.terminal_delegation_child(fact),
            "delegation.observed" => self.observe_delegation(fact),
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
            || self.delegations.values().any(|value| {
                !matches!(
                    value.state,
                    DelegationState::Denied | DelegationState::Observed
                )
            })
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
                    && self.delegation_continuation_ready(turn, prior)
            }
            _ => false,
        };
        if !valid {
            return Err(LedgerError::InvalidTransition);
        }
        self.turns.insert(turn.clone(), TurnState::Open);
        if kind == "start" {
            self.turn_agents.insert(
                turn.clone(),
                (
                    text(&payload, "agent_instance_id")?.to_owned(),
                    text(&payload, "snapshot_digest")?.to_owned(),
                ),
            );
            self.turns_started_in_commit.insert(turn.clone());
        }
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
        if text(&payload, "reason")? == "delegation_pending" {
            self.turns_suspended_in_commit.insert(turn.clone());
        }
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
        self.transition_turn(turn, next)?;
        self.turns_terminal_in_commit.insert(turn.clone());
        Ok(())
    }

    fn require_open_turn(&self, turn_id: &TurnId) -> Result<(), LedgerError> {
        if self.turns.get(turn_id) == Some(&TurnState::Open) {
            Ok(())
        } else {
            Err(LedgerError::InvalidTransition)
        }
    }

    fn admit_turn_input(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let turn = required(&fact.turn_id)?;
        let payload = payload(fact)?;
        if text(&payload, "input_kind")? == "delegation_result" {
            let suspension = text(&payload, "suspension_id")?;
            let digest = text(
                payload
                    .get("content")
                    .and_then(Value::as_object)
                    .ok_or(LedgerError::InvalidFact)?,
                "digest",
            )?;
            let record = self
                .delegations
                .values_mut()
                .find(|value| {
                    &value.parent_turn == turn
                        && value.state == DelegationState::Observed
                        && value.suspension_id.as_deref() == Some(suspension)
                        && value.result_digest.as_deref() == Some(digest)
                })
                .ok_or(LedgerError::InvalidTransition)?;
            record.input_admitted = true;
            return Ok(());
        }
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
        self.tool_contract_versions
            .insert(tool.clone(), fact.schema_version);
        if fact.schema_version == 2 {
            self.tool_result_bytes.insert(
                tool.clone(),
                payload(fact)?
                    .get("max_result_bytes")
                    .and_then(Value::as_u64)
                    .ok_or(LedgerError::InvalidFact)?,
            );
        }
        Ok(())
    }

    fn plan_effect_batch(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let execution = required(&fact.execution_id)?;
        let value = payload(fact)?;
        let plan_digest = text(&value, "plan_digest")?.to_owned();
        if self.effect_batches.contains_key(&plan_digest) {
            return Err(LedgerError::InvalidTransition);
        }
        let digests = content_value(&value, "ordered_prepared_digests")?
            .as_array()
            .cloned()
            .ok_or(LedgerError::InvalidFact)?;
        if digests.is_empty() {
            return Err(LedgerError::InvalidTransition);
        }
        let mut tools = Vec::with_capacity(digests.len());
        for digest in digests {
            let digest = digest.as_str().ok_or(LedgerError::InvalidFact)?;
            let matches: Vec<_> = self
                .tools
                .iter()
                .filter(|(tool, (owner, state))| {
                    owner == execution
                        && *state == InvocationState::Authorized
                        && self.tool_contract_versions.get(*tool) == Some(&2)
                        && self.tool_digests.get(*tool).map(String::as_str) == Some(digest)
                        && !self.tool_batch_plans.contains_key(*tool)
                })
                .map(|(tool, _)| tool.clone())
                .collect();
            let [tool] = matches.as_slice() else {
                return Err(LedgerError::InvalidTransition);
            };
            tools.push(tool.clone());
        }
        if tools.iter().collect::<BTreeSet<_>>().len() != tools.len() {
            return Err(LedgerError::InvalidTransition);
        }
        validate_batch_steps(
            &content_value(&value, "steps")?,
            &tools,
            &self.tool_result_bytes,
            value["max_parallel_reads"]
                .as_u64()
                .ok_or(LedgerError::InvalidFact)?,
            value["max_buffered_result_bytes"]
                .as_u64()
                .ok_or(LedgerError::InvalidFact)?,
        )?;
        for tool in &tools {
            self.tool_batch_plans
                .insert(tool.clone(), plan_digest.clone());
        }
        self.effect_batches.insert(
            plan_digest,
            EffectBatchRecord {
                tools,
                next_start: 0,
            },
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
        if next == InvocationState::Started && self.tool_contract_versions.get(tool) == Some(&2) {
            let plan = self
                .tool_batch_plans
                .get(tool)
                .and_then(|digest| self.effect_batches.get(digest))
                .ok_or(LedgerError::InvalidTransition)?;
            if plan.tools.get(plan.next_start) != Some(tool) {
                return Err(LedgerError::InvalidTransition);
            }
        }
        self.validate_effect_binding(tool, current, next, &payload)?;
        self.tools
            .get_mut(tool)
            .expect("validated tool remains present")
            .1 = next;
        if next == InvocationState::Started && self.tool_contract_versions.get(tool) == Some(&2) {
            let digest = self.tool_batch_plans.get(tool).expect("validated plan");
            self.effect_batches
                .get_mut(digest)
                .expect("validated plan")
                .next_start += 1;
        }
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
            execution.is_none_or(|expected| owner == expected)
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

    pub(crate) fn begin_commit(&mut self) {
        self.turns_started_in_commit.clear();
        self.turns_suspended_in_commit.clear();
        self.turns_terminal_in_commit.clear();
    }

    fn request_delegation(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let turn = required(&fact.turn_id)?.clone();
        let execution = required(&fact.execution_id)?.clone();
        let value = payload(fact)?;
        let id = text(&value, "delegation_id")?.to_owned();
        if self.delegations.contains_key(&id)
            || self.delegations.values().any(|item| {
                item.parent_turn == turn
                    && !matches!(
                        item.state,
                        DelegationState::Denied | DelegationState::Observed
                    )
            })
            || self.turn_agents.get(&turn).map(|item| item.0.as_str())
                != Some(text(&value, "parent_agent_instance_id")?)
        {
            return Err(LedgerError::InvalidTransition);
        }
        self.delegations.insert(
            id,
            DelegationRecord {
                parent_turn: turn,
                parent_execution: execution,
                intent_digest: text(&value, "intent_digest")?.to_owned(),
                grant_id: None,
                suspension_id: None,
                child_turn: None,
                result_id: None,
                result_digest: None,
                input_admitted: false,
                state: DelegationState::Requested,
            },
        );
        Ok(())
    }

    fn authorize_delegation(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let value = payload(fact)?;
        let record = self
            .delegations
            .get_mut(text(&value, "delegation_id")?)
            .ok_or(LedgerError::MissingReference)?;
        if record.state != DelegationState::Requested
            || &record.parent_turn != required(&fact.turn_id)?
            || &record.parent_execution != required(&fact.execution_id)?
            || record.intent_digest != text(&value, "intent_digest")?
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.grant_id = Some(text(&value, "grant_id")?.to_owned());
        record.state = DelegationState::Authorized;
        Ok(())
    }

    fn deny_delegation(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        self.require_active_execution(fact)?;
        let value = payload(fact)?;
        let record = self
            .delegations
            .get_mut(text(&value, "delegation_id")?)
            .ok_or(LedgerError::MissingReference)?;
        if record.state != DelegationState::Requested
            || &record.parent_turn != required(&fact.turn_id)?
            || &record.parent_execution != required(&fact.execution_id)?
            || record.intent_digest != text(&value, "intent_digest")?
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.state = DelegationState::Denied;
        Ok(())
    }

    fn start_delegation_child(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let child_turn = TurnId::try_from(text(&value, "child_turn_id")?)
            .map_err(|_| LedgerError::InvalidFact)?;
        let record = self
            .delegations
            .get_mut(text(&value, "delegation_id")?)
            .ok_or(LedgerError::MissingReference)?;
        let child = self.turn_agents.get(&child_turn);
        if record.state != DelegationState::Authorized
            || &record.parent_turn != required(&fact.turn_id)?
            || &record.parent_execution != required(&fact.execution_id)?
            || record.grant_id.as_deref() != Some(text(&value, "grant_id")?)
            || !self.turns_suspended_in_commit.contains(&record.parent_turn)
            || !self.turns_started_in_commit.contains(&child_turn)
            || child.map(|item| item.0.as_str()) != Some(text(&value, "child_agent_instance_id")?)
            || child.map(|item| item.1.as_str()) != Some(text(&value, "child_snapshot_digest")?)
        {
            return Err(LedgerError::InvalidTransition);
        }
        let suspension = text(&value, "suspension_id")?;
        if self
            .suspensions
            .get(&record.parent_turn)
            .map(String::as_str)
            != Some(suspension)
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.suspension_id = Some(suspension.to_owned());
        record.child_turn = Some(child_turn);
        record.state = DelegationState::Started;
        Ok(())
    }

    fn terminal_delegation_child(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let child_turn = TurnId::try_from(text(&value, "child_turn_id")?)
            .map_err(|_| LedgerError::InvalidFact)?;
        let record = self
            .delegations
            .get_mut(text(&value, "delegation_id")?)
            .ok_or(LedgerError::MissingReference)?;
        if record.state != DelegationState::Started
            || &record.parent_turn != required(&fact.turn_id)?
            || &record.parent_execution != required(&fact.execution_id)?
            || record.grant_id.as_deref() != Some(text(&value, "grant_id")?)
            || record.suspension_id.as_deref() != Some(text(&value, "suspension_id")?)
            || record.child_turn.as_ref() != Some(&child_turn)
            || !self.turns_terminal_in_commit.contains(&child_turn)
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.result_id = Some(text(&value, "result_id")?.to_owned());
        record.result_digest = Some(text(&value, "result_digest")?.to_owned());
        record.state = DelegationState::Terminal;
        Ok(())
    }

    fn observe_delegation(&mut self, fact: &FactDraft) -> Result<(), LedgerError> {
        let value = payload(fact)?;
        let record = self
            .delegations
            .get_mut(text(&value, "delegation_id")?)
            .ok_or(LedgerError::MissingReference)?;
        if record.state != DelegationState::Terminal
            || &record.parent_turn != required(&fact.turn_id)?
            || &record.parent_execution != required(&fact.execution_id)?
            || record.grant_id.as_deref() != Some(text(&value, "grant_id")?)
            || record.suspension_id.as_deref() != Some(text(&value, "suspension_id")?)
            || record.result_id.as_deref() != Some(text(&value, "result_id")?)
            || record.result_digest.as_deref() != Some(text(&value, "result_digest")?)
        {
            return Err(LedgerError::InvalidTransition);
        }
        record.state = DelegationState::Observed;
        Ok(())
    }

    fn delegation_continuation_ready(&self, turn: &TurnId, suspension: Option<&str>) -> bool {
        let matching: Vec<_> = self
            .delegations
            .values()
            .filter(|item| &item.parent_turn == turn && item.suspension_id.as_deref() == suspension)
            .collect();
        matching.is_empty()
            || matching
                .iter()
                .all(|item| item.state == DelegationState::Observed && item.input_admitted)
    }

    pub(crate) fn validate_commit_boundary(&self) -> Result<(), LedgerError> {
        if self
            .schedules
            .values()
            .any(|value| value.state == ScheduleState::Superseding)
            || self.delegations.values().any(|value| {
                (self.turns_suspended_in_commit.contains(&value.parent_turn)
                    && value.state == DelegationState::Authorized)
                    || value.child_turn.as_ref().is_some_and(|turn| {
                        self.turns_terminal_in_commit.contains(turn)
                            && value.state == DelegationState::Started
                    })
            })
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

fn content_value(value: &Map<String, Value>, key: &str) -> Result<Value, LedgerError> {
    let binding = value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(LedgerError::InvalidFact)?;
    let canonical = CanonicalPayload::from_canonical_parts(
        text(binding, "inline_utf8")?.to_owned(),
        text(binding, "digest")?.to_owned(),
    )
    .map_err(|_| LedgerError::InvalidFact)?;
    serde_json::from_str(canonical.as_json()).map_err(|_| LedgerError::InvalidFact)
}

fn validate_batch_steps(
    value: &Value,
    tools: &[ToolInvocationId],
    result_bytes: &BTreeMap<ToolInvocationId, u64>,
    max_parallel: u64,
    max_buffered: u64,
) -> Result<(), LedgerError> {
    let steps = value.as_array().ok_or(LedgerError::InvalidFact)?;
    let mut indexes = Vec::new();
    for step in steps {
        let step = step.as_object().ok_or(LedgerError::InvalidFact)?;
        match text(step, "kind")? {
            "sequential_step" if step.len() == 2 => indexes.push(
                step.get("intent_index")
                    .and_then(Value::as_u64)
                    .ok_or(LedgerError::InvalidFact)? as usize,
            ),
            "parallel_read_group" if step.len() == 2 => {
                let group = step
                    .get("intent_indexes")
                    .and_then(Value::as_array)
                    .ok_or(LedgerError::InvalidFact)?;
                if group.is_empty() || group.len() as u64 > max_parallel {
                    return Err(LedgerError::InvalidTransition);
                }
                let mut bytes = 0u64;
                for index in group {
                    let index = index.as_u64().ok_or(LedgerError::InvalidFact)? as usize;
                    bytes = bytes
                        .checked_add(
                            *result_bytes
                                .get(tools.get(index).ok_or(LedgerError::InvalidTransition)?)
                                .ok_or(LedgerError::InvalidTransition)?,
                        )
                        .ok_or(LedgerError::InvalidTransition)?;
                    indexes.push(index);
                }
                if bytes > max_buffered {
                    return Err(LedgerError::InvalidTransition);
                }
            }
            _ => return Err(LedgerError::InvalidFact),
        }
    }
    if indexes != (0..tools.len()).collect::<Vec<_>>() {
        return Err(LedgerError::InvalidTransition);
    }
    Ok(())
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
