//! Pure deterministic C5b conflict graph and batch planner.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::access::{AccessMode, ResourceAccess};
use crate::prepared::{PreparedToolCall, ReplayClass};

/// Stable pure planner failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectBatchErrorCode {
    /// A call is not Prepared v2 or lacks a valid exact access set.
    EffectAccessInvalid,
    /// An input, group, access, or buffer bound is exceeded.
    EffectBatchBoundExceeded,
}

/// Typed deterministic C5b planner failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectBatchError {
    code: EffectBatchErrorCode,
}

impl EffectBatchError {
    /// Returns the stable failure classification.
    pub const fn code(self) -> EffectBatchErrorCode {
        self.code
    }
}

/// Explicit non-zero planner and group bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectBatchLimitsV1 {
    max_intents: usize,
    max_accesses_per_intent: usize,
    max_total_accesses: usize,
    max_parallel_reads: usize,
    max_buffered_result_bytes: u64,
}

impl EffectBatchLimitsV1 {
    /// Validates and constructs the complete v1 limit snapshot.
    pub const fn new(
        max_intents: usize,
        max_accesses_per_intent: usize,
        max_total_accesses: usize,
        max_parallel_reads: usize,
        max_buffered_result_bytes: u64,
    ) -> Result<Self, EffectBatchError> {
        if max_intents == 0
            || max_accesses_per_intent == 0
            || max_total_accesses == 0
            || max_parallel_reads == 0
            || max_buffered_result_bytes == 0
        {
            return Err(bound_error());
        }
        Ok(Self {
            max_intents,
            max_accesses_per_intent,
            max_total_accesses,
            max_parallel_reads,
            max_buffered_result_bytes,
        })
    }
}

/// One ordered deterministic execution step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EffectBatchStep {
    /// One call that Runtime must execute sequentially.
    SequentialStep {
        /// Original zero-based model intent index.
        intent_index: usize,
    },
    /// One contiguous bounded non-conflicting read-only group.
    ParallelReadGroup {
        /// Increasing original zero-based model intent indexes.
        intent_indexes: Vec<usize>,
    },
}

/// One Prepared Call plus an admitted interaction boundary decision.
#[derive(Clone, Copy, Debug)]
pub struct EffectBatchIntent<'a> {
    prepared: &'a PreparedToolCall,
    suspension_boundary: bool,
}

impl<'a> EffectBatchIntent<'a> {
    /// Binds one Prepared Call and whether it may suspend before dispatch.
    pub const fn new(prepared: &'a PreparedToolCall, suspension_boundary: bool) -> Self {
        Self {
            prepared,
            suspension_boundary,
        }
    }
}

/// Canonical conflict graph and ordered execution plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectBatchPlanV1 {
    ordered_prepared_digests: Vec<String>,
    conflict_graph_bytes: Vec<u8>,
    conflict_graph_digest: String,
    steps: Vec<EffectBatchStep>,
    plan_digest: String,
}

impl EffectBatchPlanV1 {
    /// Returns ordered Prepared Call digests.
    pub fn ordered_prepared_digests(&self) -> &[String] {
        &self.ordered_prepared_digests
    }

    /// Returns upper-triangle graph bytes in ascending index-pair order.
    pub fn conflict_graph_bytes(&self) -> &[u8] {
        &self.conflict_graph_bytes
    }

    /// Returns lowercase SHA-256 of the graph bytes.
    pub fn conflict_graph_digest(&self) -> &str {
        &self.conflict_graph_digest
    }

    /// Returns the ordered complete plan steps.
    pub fn steps(&self) -> &[EffectBatchStep] {
        &self.steps
    }

    /// Returns lowercase SHA-256 of the canonical plan without this field.
    pub fn plan_digest(&self) -> &str {
        &self.plan_digest
    }
}

/// Plans Prepared v2 calls with no suspension boundaries.
pub fn plan_effect_batch(
    prepared: &[PreparedToolCall],
    limits: &EffectBatchLimitsV1,
) -> Result<EffectBatchPlanV1, EffectBatchError> {
    let intents: Vec<_> = prepared
        .iter()
        .map(|call| EffectBatchIntent::new(call, false))
        .collect();
    plan_effect_batch_intents(&intents, limits)
}

/// Plans Prepared v2 calls with explicit admitted suspension boundaries.
pub fn plan_effect_batch_intents(
    intents: &[EffectBatchIntent<'_>],
    limits: &EffectBatchLimitsV1,
) -> Result<EffectBatchPlanV1, EffectBatchError> {
    if intents.is_empty() || intents.len() > limits.max_intents {
        return Err(bound_error());
    }
    let mut total_accesses = 0usize;
    for intent in intents {
        let call = intent.prepared;
        let accesses = call.invocation_accesses().ok_or_else(access_error)?;
        if call.contract_version() != 2 || accesses.values().is_empty() {
            return Err(access_error());
        }
        if accesses.values().len() > limits.max_accesses_per_intent {
            return Err(bound_error());
        }
        total_accesses = total_accesses
            .checked_add(accesses.values().len())
            .ok_or_else(bound_error)?;
    }
    if total_accesses > limits.max_total_accesses {
        return Err(bound_error());
    }

    let graph = conflict_graph(intents);
    let mut steps = Vec::new();
    let mut group = Vec::new();
    let mut group_accesses = 0usize;
    let mut group_bytes = 0u64;
    for (index, intent) in intents.iter().enumerate() {
        let call = intent.prepared;
        let accesses = call.invocation_accesses().ok_or_else(access_error)?;
        let result_bytes = call.max_result_bytes().ok_or_else(access_error)?;
        let conflicts = group
            .iter()
            .any(|member| graph_edge(&graph, intents.len(), *member, index));
        let next_accesses = group_accesses + accesses.values().len();
        let next_bytes = group_bytes
            .checked_add(result_bytes)
            .ok_or_else(bound_error)?;
        let read_group = call.replay_class() == ReplayClass::ReadOnly
            && !intent.suspension_boundary
            && !conflicts
            && group.len() < limits.max_parallel_reads
            && next_accesses <= limits.max_total_accesses
            && next_bytes <= limits.max_buffered_result_bytes;
        if read_group {
            group.push(index);
            group_accesses = next_accesses;
            group_bytes = next_bytes;
        } else {
            flush_group(&mut steps, &mut group);
            group_accesses = 0;
            group_bytes = 0;
            if call.replay_class() == ReplayClass::ReadOnly && !intent.suspension_boundary {
                if result_bytes > limits.max_buffered_result_bytes {
                    return Err(bound_error());
                }
                group.push(index);
                group_accesses = accesses.values().len();
                group_bytes = result_bytes;
            } else {
                steps.push(EffectBatchStep::SequentialStep {
                    intent_index: index,
                });
            }
        }
    }
    flush_group(&mut steps, &mut group);

    let ordered_prepared_digests = intents
        .iter()
        .map(|intent| intent.prepared.input_digest().to_owned())
        .collect::<Vec<_>>();
    let conflict_graph_digest = sha256(&graph);
    let preimage = json!({
        "schema_version": 1,
        "prepared_contract_version": 2,
        "ordered_prepared_digests": ordered_prepared_digests,
        "conflict_graph_digest": conflict_graph_digest,
        "steps": steps,
    });
    let canonical = serde_jcs::to_vec(&preimage).map_err(|_| access_error())?;
    Ok(EffectBatchPlanV1 {
        ordered_prepared_digests,
        conflict_graph_bytes: graph,
        conflict_graph_digest,
        steps,
        plan_digest: sha256(&canonical),
    })
}

fn conflict_graph(intents: &[EffectBatchIntent<'_>]) -> Vec<u8> {
    let mut graph = Vec::with_capacity(intents.len().saturating_mul(intents.len()) / 2);
    for left in 0..intents.len() {
        for right in (left + 1)..intents.len() {
            let left_accesses = intents[left]
                .prepared
                .invocation_accesses()
                .unwrap()
                .values();
            let right_accesses = intents[right]
                .prepared
                .invocation_accesses()
                .unwrap()
                .values();
            graph.push(u8::from(left_accesses.iter().any(|left_access| {
                right_accesses
                    .iter()
                    .any(|right_access| accesses_conflict(left_access, right_access))
            })));
        }
    }
    graph
}

fn accesses_conflict(left: &ResourceAccess, right: &ResourceAccess) -> bool {
    left.namespace() == right.namespace()
        && (left.mode() == AccessMode::Exclusive
            || right.mode() == AccessMode::Exclusive
            || (left.resource_key() == right.resource_key()
                && (left.mode() == AccessMode::Write || right.mode() == AccessMode::Write)))
}

fn graph_edge(graph: &[u8], count: usize, left: usize, right: usize) -> bool {
    let offset = left * (2 * count - left - 1) / 2 + (right - left - 1);
    graph[offset] == 1
}

fn flush_group(steps: &mut Vec<EffectBatchStep>, group: &mut Vec<usize>) {
    if !group.is_empty() {
        steps.push(EffectBatchStep::ParallelReadGroup {
            intent_indexes: std::mem::take(group),
        });
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

const fn access_error() -> EffectBatchError {
    EffectBatchError {
        code: EffectBatchErrorCode::EffectAccessInvalid,
    }
}

const fn bound_error() -> EffectBatchError {
    EffectBatchError {
        code: EffectBatchErrorCode::EffectBatchBoundExceeded,
    }
}
