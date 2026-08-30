//! SQLite publication adapter for deterministic C5b execution.

use garive_ledger::{CanonicalPayload, DurableFact, SessionId};
use garive_tools::{EffectReceipt, ExecutionFact, TerminalClassification};
use serde_json::{json, Map, Value};

use crate::{
    effect_batch_facts::{batch_fact, child_id, content},
    AuthorizedBatchInvocation, BatchRuntimeError, BatchTerminal, EffectBatchAdmissionContext,
    EffectBatchPublisher, PreparedExecution, SqliteLedger,
};

/// SQLite-backed ordered terminal publisher for one committed C5b plan.
pub struct SqliteEffectBatchPublisher<'a> {
    ledger: &'a mut SqliteLedger,
    session_id: SessionId,
    version: u64,
    context: EffectBatchAdmissionContext,
    plan_digest: String,
}

impl<'a> SqliteEffectBatchPublisher<'a> {
    /// Opens a publisher only when the exact plan is durable under an active Execution.
    pub fn new(
        ledger: &'a mut SqliteLedger,
        session_id: SessionId,
        expected_session_version: u64,
        context: EffectBatchAdmissionContext,
        plan_digest: impl Into<String>,
    ) -> Result<Self, BatchRuntimeError> {
        let plan_digest = plan_digest.into();
        let snapshot = ledger
            .load_turn(&context.turn_id)
            .map_err(|_| BatchRuntimeError::DurabilityFailure)?;
        if snapshot.session_version != expected_session_version
            || snapshot
                .facts
                .iter()
                .any(|fact| fact.session_id != session_id)
            || !execution_is_active(&snapshot.facts, &context.execution_id)
            || !has_exact_plan(&snapshot.facts, &context, &plan_digest)
        {
            return Err(BatchRuntimeError::InvalidBinding);
        }
        Ok(Self {
            ledger,
            session_id,
            version: expected_session_version,
            context,
            plan_digest,
        })
    }

    /// Returns the latest Session version after ordered publication.
    pub const fn session_version(&self) -> u64 {
        self.version
    }

    fn commit(&mut self, facts: Vec<garive_ledger::FactDraft>) -> Result<(), BatchRuntimeError> {
        let result = self
            .ledger
            .commit(self.session_id.clone(), self.version, facts)
            .map_err(|_| BatchRuntimeError::DurabilityFailure)?;
        self.version = result.session_version;
        Ok(())
    }

    fn execution_terminal_facts(
        &self,
        invocation: &AuthorizedBatchInvocation,
        execution: &PreparedExecution,
        fact: &ExecutionFact,
    ) -> Result<Vec<garive_ledger::FactDraft>, BatchRuntimeError> {
        let receipt = terminal_receipt(fact).ok_or(BatchRuntimeError::InvalidBinding)?;
        validate_receipt(invocation, execution, receipt, fact)?;
        let evidence = terminal_evidence(fact)?;
        let receipt_fact = batch_fact(
            &self.context,
            &child_id(
                &self.plan_digest,
                invocation.invocation_id.as_str(),
                "receipt",
            ),
            "effect.receipt",
            1,
            Some(invocation.invocation_id.as_str()),
            json!({
                "receipt_id":receipt.receipt_id.as_str(),
                "prepared_digest":invocation.prepared.input_digest(),
                "grant_id":receipt.grant_id.as_str(),
                "executor_id":receipt.executor_id,
                "executor_revision":receipt.executor_revision,
                "classification":terminal_class(receipt.terminal_classification),
                "result_or_evidence":content(&evidence)?,
            }),
        )?;
        let (kind, terminal_payload, envelope) = match fact {
            ExecutionFact::Completed {
                content: value,
                truncated,
                ..
            } => (
                "effect.completed",
                json!({
                    "prepared_digest":invocation.prepared.input_digest(),
                    "receipt_id":receipt.receipt_id.as_str(),
                    "result":content(value)?,
                }),
                json!({"status":"succeeded","content":value,"truncated":truncated}),
            ),
            ExecutionFact::Failed {
                code,
                details,
                partial,
                ..
            } => {
                let stable = stable_failure(code);
                let mut payload = Map::from_iter([
                    (
                        "prepared_digest".into(),
                        json!(invocation.prepared.input_digest()),
                    ),
                    ("receipt_id".into(), json!(receipt.receipt_id.as_str())),
                    ("code".into(), json!(stable)),
                ]);
                if details.is_some() || partial.is_some() {
                    payload.insert(
                        "evidence".into(),
                        content(&json!({"details":details,"partial":partial}))?,
                    );
                }
                (
                    "effect.failed",
                    Value::Object(payload),
                    json!({"status":"failed","code":stable,"details":details,"partial":partial}),
                )
            }
            _ => return Err(BatchRuntimeError::InvalidBinding),
        };
        Ok(vec![
            receipt_fact,
            batch_fact(
                &self.context,
                &child_id(
                    &self.plan_digest,
                    invocation.invocation_id.as_str(),
                    "terminal",
                ),
                kind,
                1,
                Some(invocation.invocation_id.as_str()),
                terminal_payload,
            )?,
            self.observation_fact(invocation, envelope)?,
        ])
    }

    fn failure_facts(
        &self,
        invocation: &AuthorizedBatchInvocation,
        terminal: &BatchTerminal,
    ) -> Result<Vec<garive_ledger::FactDraft>, BatchRuntimeError> {
        let code = match terminal {
            BatchTerminal::ExecutionTimedOut => "effect_execution_timeout",
            BatchTerminal::Cancelled => "cancelled",
            BatchTerminal::ResultBoundExceeded => "effect_batch_bound_exceeded",
            _ => return Err(BatchRuntimeError::InvalidBinding),
        };
        Ok(vec![
            batch_fact(
                &self.context,
                &child_id(
                    &self.plan_digest,
                    invocation.invocation_id.as_str(),
                    "terminal",
                ),
                "effect.failed",
                1,
                Some(invocation.invocation_id.as_str()),
                json!({
                    "prepared_digest":invocation.prepared.input_digest(),
                    "code":code,
                    "evidence":content(&Value::String(code.into()))?,
                }),
            )?,
            self.observation_fact(
                invocation,
                json!({"status":"failed","code":code,"details":null,"partial":null}),
            )?,
        ])
    }

    fn observation_fact(
        &self,
        invocation: &AuthorizedBatchInvocation,
        envelope: Value,
    ) -> Result<garive_ledger::FactDraft, BatchRuntimeError> {
        batch_fact(
            &self.context,
            &child_id(
                &self.plan_digest,
                invocation.invocation_id.as_str(),
                "observation",
            ),
            "effect.observation",
            1,
            Some(invocation.invocation_id.as_str()),
            json!({
                "prepared_digest":invocation.prepared.input_digest(),
                "model_call_id":invocation.prepared.model_call_id(),
                "observation":content(&envelope)?,
            }),
        )
    }
}

impl EffectBatchPublisher for SqliteEffectBatchPublisher<'_> {
    fn commit_started(
        &mut self,
        _: usize,
        invocation: &AuthorizedBatchInvocation,
        execution: &PreparedExecution,
    ) -> Result<(), BatchRuntimeError> {
        validate_execution(execution)?;
        let fact = batch_fact(
            &self.context,
            &child_id(
                &self.plan_digest,
                invocation.invocation_id.as_str(),
                "started",
            ),
            "effect.started",
            1,
            Some(invocation.invocation_id.as_str()),
            json!({
                "prepared_digest":invocation.prepared.input_digest(),
                "grant_id":invocation.grant.grant_id.as_str(),
                "executor_id":execution.executor_id,
                "executor_revision":execution.executor_revision,
                "dispatch_attempt_id":execution.dispatch_attempt_id,
            }),
        )?;
        self.commit(vec![fact])
    }

    fn publish_terminal(
        &mut self,
        _: usize,
        invocation: &AuthorizedBatchInvocation,
        execution: &PreparedExecution,
        terminal: &BatchTerminal,
    ) -> Result<(), BatchRuntimeError> {
        let facts = match terminal {
            BatchTerminal::Execution(fact) => {
                self.execution_terminal_facts(invocation, execution, fact)?
            }
            BatchTerminal::Uncertain => vec![batch_fact(
                &self.context,
                &child_id(
                    &self.plan_digest,
                    invocation.invocation_id.as_str(),
                    "uncertain",
                ),
                "effect.uncertain",
                1,
                Some(invocation.invocation_id.as_str()),
                json!({
                    "prepared_digest":invocation.prepared.input_digest(),
                    "reason":"executor_state_unknown",
                    "evidence":content(&Value::String("effect_batch_uncertain".into()))?,
                }),
            )?],
            value => self.failure_facts(invocation, value)?,
        };
        self.commit(facts)
    }
}

fn has_exact_plan(
    facts: &[DurableFact],
    context: &EffectBatchAdmissionContext,
    plan_digest: &str,
) -> bool {
    facts.iter().any(|fact| {
        fact.execution_id.as_ref() == Some(&context.execution_id)
            && fact.kind.as_str() == "execution.effect_batch_planned"
            && serde_json::from_str::<Value>(fact.payload.as_json()).is_ok_and(|value| {
                value["plan_digest"] == plan_digest
                    && value["max_parallel_reads"] == context.max_parallel_reads
                    && value["max_buffered_result_bytes"] == context.max_buffered_result_bytes
            })
    })
}

fn execution_is_active(facts: &[DurableFact], execution_id: &garive_ledger::ExecutionId) -> bool {
    let mut active = false;
    for fact in facts
        .iter()
        .filter(|fact| fact.execution_id.as_ref() == Some(execution_id))
    {
        match fact.kind.as_str() {
            "execution.started" => active = true,
            "execution.abandoned"
            | "execution.completed"
            | "execution.suspended"
            | "execution.stopped"
            | "execution.failed" => active = false,
            _ => {}
        }
    }
    active
}

fn validate_execution(value: &PreparedExecution) -> Result<(), BatchRuntimeError> {
    if value.executor_id.is_empty()
        || value.executor_revision.is_empty()
        || value.dispatch_attempt_id.is_empty()
    {
        Err(BatchRuntimeError::InvalidBinding)
    } else {
        Ok(())
    }
}

fn validate_receipt(
    invocation: &AuthorizedBatchInvocation,
    execution: &PreparedExecution,
    receipt: &EffectReceipt,
    fact: &ExecutionFact,
) -> Result<(), BatchRuntimeError> {
    receipt
        .validate()
        .map_err(|_| BatchRuntimeError::InvalidBinding)?;
    let canonical = CanonicalPayload::from_value(&terminal_evidence(fact)?)
        .map_err(|_| BatchRuntimeError::InvalidBinding)?;
    if receipt.receipt_id.as_str() != invocation.receipt_id
        || receipt.invocation_id != invocation.invocation_id
        || receipt.prepared_digest != invocation.prepared.input_digest()
        || receipt.grant_id != invocation.grant.grant_id
        || receipt.executor_id != execution.executor_id
        || receipt.executor_revision != execution.executor_revision
        || receipt.result_digest != canonical.sha256()
    {
        return Err(BatchRuntimeError::InvalidBinding);
    }
    match (fact, receipt.terminal_classification) {
        (ExecutionFact::Completed { .. }, TerminalClassification::Completed)
        | (ExecutionFact::Failed { .. }, TerminalClassification::Failed) => Ok(()),
        _ => Err(BatchRuntimeError::InvalidBinding),
    }
}

fn terminal_receipt(fact: &ExecutionFact) -> Option<&EffectReceipt> {
    match fact {
        ExecutionFact::Completed { receipt, .. } | ExecutionFact::Failed { receipt, .. } => {
            receipt.as_ref()
        }
        _ => None,
    }
}

fn terminal_evidence(fact: &ExecutionFact) -> Result<Value, BatchRuntimeError> {
    match fact {
        ExecutionFact::Completed { content, .. } => Ok(content.clone()),
        ExecutionFact::Failed {
            code,
            details,
            partial,
            ..
        } => Ok(json!({"code":code,"details":details,"partial":partial})),
        _ => Err(BatchRuntimeError::InvalidBinding),
    }
}

const fn terminal_class(value: TerminalClassification) -> &'static str {
    match value {
        TerminalClassification::Completed => "completed",
        TerminalClassification::Failed => "failed",
    }
}

fn stable_failure(value: &str) -> &'static str {
    match value {
        "timeout" => "timeout",
        "cancelled" => "cancelled",
        "executor_unavailable" => "executor_unavailable",
        _ => "tool_failure",
    }
}
