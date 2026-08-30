//! Bounded C5b dispatch with deterministic durable publication order.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use futures::{stream::FuturesUnordered, StreamExt};
use garive_tools::{
    EffectBatchPlanV1, EffectBatchStep, ExecutionFact, InvocationGrant, PreparedToolCall,
    ToolInvocationId,
};
use tokio::sync::{Notify, Semaphore};

use crate::{ExecutorDispatchError, PreparedExecution};

/// Frozen authorized invocation admitted to one committed batch plan.
#[derive(Clone, Debug)]
pub struct AuthorizedBatchInvocation {
    /// Runtime-owned invocation identity.
    pub invocation_id: ToolInvocationId,
    /// Exact Prepared v2 call.
    pub prepared: PreparedToolCall,
    /// Exact authority grant bound to the Prepared digest.
    pub grant: InvocationGrant,
    /// Runtime-owned receipt identity expected from the executor.
    pub receipt_id: String,
}

/// Owned executor command that remains valid across concurrent polling.
#[derive(Clone, Debug)]
pub struct ConcurrentExecutorDispatch {
    /// Runtime-owned invocation identity.
    pub invocation_id: ToolInvocationId,
    /// Exact Prepared v2 call.
    pub prepared: PreparedToolCall,
    /// Exact frozen authority grant.
    pub grant: InvocationGrant,
    /// Executor binding proven before Started.
    pub execution: PreparedExecution,
    /// Required terminal receipt identity.
    pub receipt_id: String,
}

/// Cooperative cancellation signal scoped to one executor invocation.
#[derive(Clone, Default)]
pub struct EffectCancellation(Arc<CancellationState>);

#[derive(Default)]
struct CancellationState {
    cancelled: std::sync::atomic::AtomicBool,
    notify: Notify,
}

impl EffectCancellation {
    /// Requests cancellation idempotently.
    pub fn cancel(&self) {
        self.0
            .cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        self.0.notify.notify_waiters();
    }

    /// Returns whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(std::sync::atomic::Ordering::Acquire)
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.0.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

/// Evidence obtained after cooperative cancellation and its grace period.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CancellationEvidence {
    /// Executor proves a trustworthy terminal result.
    Terminal(Box<ExecutionFact>),
    /// Executor proves the invocation did not complete.
    ProvenNotCompleted,
    /// Executor cannot prove either state.
    Unknown,
}

/// Executor boundary suitable for concurrently polled, owned dispatches.
pub trait ConcurrentExecutorPort: Send + Sync {
    /// Proves requirements and selects the exact executor without dispatching.
    fn prepare(&self, invocation: &AuthorizedBatchInvocation) -> Result<PreparedExecution, String>;

    /// Dispatches only after the matching Started commit succeeds.
    fn dispatch<'a>(
        &'a self,
        command: ConcurrentExecutorDispatch,
        cancellation: EffectCancellation,
    ) -> Pin<Box<dyn Future<Output = Result<ExecutionFact, ExecutorDispatchError>> + Send + 'a>>;

    /// Returns bounded cancellation evidence after grace expiry.
    fn cancellation_evidence(&self, invocation_id: &ToolInvocationId) -> CancellationEvidence;
}

/// Normalized terminal retained and published by model order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchTerminal {
    /// Trustworthy executor terminal within bounds.
    Execution(Box<ExecutionFact>),
    /// Executor proved non-completion after the invocation deadline.
    ExecutionTimedOut,
    /// Executor proved non-completion after Turn cancellation.
    Cancelled,
    /// Started invocation requires reconciliation.
    Uncertain,
    /// Executor exceeded the Prepared Call result charge.
    ResultBoundExceeded,
}

/// Durable publication boundary owned by the Runtime ledger adapter.
pub trait EffectBatchPublisher {
    /// Commits Started before the corresponding executor is polled.
    fn commit_started(
        &mut self,
        model_index: usize,
        invocation: &AuthorizedBatchInvocation,
        execution: &PreparedExecution,
    ) -> Result<(), BatchRuntimeError>;

    /// Commits terminal/reconciliation and observation in model order.
    fn publish_terminal(
        &mut self,
        model_index: usize,
        invocation: &AuthorizedBatchInvocation,
        execution: &PreparedExecution,
        terminal: &BatchTerminal,
    ) -> Result<(), BatchRuntimeError>;
}

/// Explicit non-zero dispatcher bounds.
#[derive(Clone, Copy, Debug)]
pub struct EffectBatchRuntimeLimits {
    /// Maximum dispatches concurrently polling across batches.
    pub max_parallel_reads: usize,
    /// Maximum wait for a dispatcher permit.
    pub queue_timeout: Duration,
    /// Per-invocation duration after durable Started.
    pub invocation_timeout: Duration,
    /// Bounded cooperative-cancellation grace period.
    pub cancellation_grace: Duration,
}

/// Stable failure of batch admission or durable execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchRuntimeError {
    /// Plan, grant, digest, limit, or executor preflight binding is invalid.
    InvalidBinding,
    /// A step could not acquire capacity before its queue deadline.
    QueueTimeout,
    /// Started or ordered terminal publication failed.
    DurabilityFailure,
}

/// Successful ordered batch execution summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectBatchReport {
    /// Model indexes whose Started facts committed.
    pub started_indexes: Vec<usize>,
    /// Terminals published in the same model order.
    pub terminals: Vec<(usize, BatchTerminal)>,
}

/// Reusable bounded dispatcher; its semaphore also bounds concurrent batches.
pub struct EffectBatchDispatcher {
    permits: Arc<Semaphore>,
    max_parallel_reads: usize,
}

impl EffectBatchDispatcher {
    /// Creates a dispatcher from the complete non-zero Runtime limit snapshot.
    pub fn new(limits: EffectBatchRuntimeLimits) -> Result<Self, BatchRuntimeError> {
        if limits.max_parallel_reads == 0
            || limits.queue_timeout.is_zero()
            || limits.invocation_timeout.is_zero()
            || limits.cancellation_grace.is_zero()
        {
            return Err(BatchRuntimeError::InvalidBinding);
        }
        Ok(Self {
            permits: Arc::new(Semaphore::new(limits.max_parallel_reads)),
            max_parallel_reads: limits.max_parallel_reads,
        })
    }

    /// Executes one already-committed plan and publishes every started member in model order.
    pub async fn execute(
        &self,
        plan: &EffectBatchPlanV1,
        invocations: &[AuthorizedBatchInvocation],
        limits: EffectBatchRuntimeLimits,
        cancellation: &EffectCancellation,
        executor: &dyn ConcurrentExecutorPort,
        publisher: &mut dyn EffectBatchPublisher,
    ) -> Result<EffectBatchReport, BatchRuntimeError> {
        if limits.max_parallel_reads > self.max_parallel_reads {
            return Err(BatchRuntimeError::InvalidBinding);
        }
        validate_inputs(plan, invocations, limits)?;
        let mut report = EffectBatchReport {
            started_indexes: Vec::new(),
            terminals: Vec::new(),
        };
        for step in plan.steps() {
            if cancellation.is_cancelled() {
                break;
            }
            let indexes: Vec<_> = match step {
                EffectBatchStep::SequentialStep { intent_index } => vec![*intent_index],
                EffectBatchStep::ParallelReadGroup { intent_indexes } => intent_indexes.clone(),
            };
            let mut running = FuturesUnordered::new();
            for index in indexes {
                if cancellation.is_cancelled() {
                    break;
                }
                let permit = tokio::time::timeout(
                    limits.queue_timeout,
                    self.permits.clone().acquire_owned(),
                )
                .await
                .map_err(|_| BatchRuntimeError::QueueTimeout)?
                .map_err(|_| BatchRuntimeError::InvalidBinding)?;
                let invocation = &invocations[index];
                let execution = executor
                    .prepare(invocation)
                    .map_err(|_| BatchRuntimeError::InvalidBinding)?;
                publisher
                    .commit_started(index, invocation, &execution)
                    .map_err(|_| BatchRuntimeError::DurabilityFailure)?;
                report.started_indexes.push(index);
                let child = EffectCancellation::default();
                running.push(run_invocation(
                    index,
                    invocation,
                    execution,
                    child,
                    cancellation,
                    limits,
                    executor,
                    permit,
                ));
            }
            let mut buffered = vec![None; invocations.len()];
            while let Some((index, execution, terminal)) = running.next().await {
                buffered[index] = Some((execution, terminal));
            }
            for index in report.started_indexes.iter().copied() {
                if report
                    .terminals
                    .iter()
                    .any(|(published, _)| *published == index)
                {
                    continue;
                }
                let Some((execution, terminal)) = buffered[index].take() else {
                    continue;
                };
                publisher
                    .publish_terminal(index, &invocations[index], &execution, &terminal)
                    .map_err(|_| BatchRuntimeError::DurabilityFailure)?;
                report.terminals.push((index, terminal));
            }
        }
        Ok(report)
    }
}

fn validate_inputs(
    plan: &EffectBatchPlanV1,
    invocations: &[AuthorizedBatchInvocation],
    limits: EffectBatchRuntimeLimits,
) -> Result<(), BatchRuntimeError> {
    if invocations.len() != plan.ordered_prepared_digests().len()
        || limits.max_parallel_reads == 0
        || limits.queue_timeout.is_zero()
        || limits.invocation_timeout.is_zero()
        || limits.cancellation_grace.is_zero()
    {
        return Err(BatchRuntimeError::InvalidBinding);
    }
    for (invocation, digest) in invocations.iter().zip(plan.ordered_prepared_digests()) {
        if invocation.prepared.contract_version() != 2
            || invocation.prepared.input_digest() != digest
            || invocation.grant.invocation_id != invocation.invocation_id
            || invocation.grant.prepared_digest != *digest
            || invocation.receipt_id.is_empty()
        {
            return Err(BatchRuntimeError::InvalidBinding);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_invocation(
    index: usize,
    invocation: &AuthorizedBatchInvocation,
    execution: PreparedExecution,
    child: EffectCancellation,
    batch_cancellation: &EffectCancellation,
    limits: EffectBatchRuntimeLimits,
    executor: &dyn ConcurrentExecutorPort,
    _permit: tokio::sync::OwnedSemaphorePermit,
) -> (usize, PreparedExecution, BatchTerminal) {
    let command = ConcurrentExecutorDispatch {
        invocation_id: invocation.invocation_id.clone(),
        prepared: invocation.prepared.clone(),
        grant: invocation.grant.clone(),
        execution: execution.clone(),
        receipt_id: invocation.receipt_id.clone(),
    };
    let mut dispatch = executor.dispatch(command, child.clone());
    let cancelled = tokio::select! {
        result = &mut dispatch => return (index, execution, normalize(result, invocation)),
        _ = tokio::time::sleep(limits.invocation_timeout) => BatchTerminal::ExecutionTimedOut,
        _ = batch_cancellation.cancelled() => BatchTerminal::Cancelled,
    };
    child.cancel();
    if let Ok(result) = tokio::time::timeout(limits.cancellation_grace, &mut dispatch).await {
        return (index, execution, normalize(result, invocation));
    }
    let terminal = match executor.cancellation_evidence(&invocation.invocation_id) {
        CancellationEvidence::Terminal(value) => normalize(Ok(*value), invocation),
        CancellationEvidence::ProvenNotCompleted => cancelled,
        CancellationEvidence::Unknown => BatchTerminal::Uncertain,
    };
    (index, execution, terminal)
}

fn normalize(
    result: Result<ExecutionFact, ExecutorDispatchError>,
    invocation: &AuthorizedBatchInvocation,
) -> BatchTerminal {
    let Ok(fact) = result else {
        return BatchTerminal::Uncertain;
    };
    let bytes = match &fact {
        ExecutionFact::Completed { content, .. } => serde_jcs::to_vec(content).map(|v| v.len()),
        ExecutionFact::Failed { partial, .. } => partial
            .as_ref()
            .map_or(Ok(0), |value| serde_jcs::to_vec(value).map(|v| v.len())),
        _ => return BatchTerminal::Uncertain,
    };
    if bytes.ok().and_then(|value| u64::try_from(value).ok())
        > invocation.prepared.max_result_bytes()
    {
        BatchTerminal::ResultBoundExceeded
    } else {
        BatchTerminal::Execution(Box::new(fact))
    }
}
