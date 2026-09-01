use std::collections::BTreeMap;

use garive_ledger::{CanonicalPayload, FactDraft};
use garive_plan::{PlanSnapshot, PlanStepId};

mod command;

pub use command::{
    commit_plan_command, plan_adopt_plan, plan_plan_transition, plan_propose_plan,
    plan_start_step_execution,
};

/// Authenticated metadata bound to one idempotent Plan command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanCommandContext {
    /// Stable command identity, reused only for exact replay.
    pub command_id: String,
    /// Authenticated actor or proposer reference.
    pub actor_reference: String,
    /// RFC 3339 observation timestamp.
    pub recorded_at: String,
}

/// One reconstructed fenced claim, including started-attempt ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivePlanClaim {
    /// Stable claim identity.
    pub claim_id: String,
    /// Worker currently holding the claim.
    pub worker_reference: String,
    /// Positive fencing epoch.
    pub lease_epoch: u64,
    /// Monotonic clock implementation revision.
    pub clock_revision: String,
    /// Inclusive claim tick.
    pub claimed_at_tick: u64,
    /// Exclusive expiry tick.
    pub expires_at_tick: u64,
    /// Attempt identity after start.
    pub attempt_id: Option<String>,
    /// C6 Execution identity after start.
    pub execution_id: Option<String>,
}

/// Verified Plan projection at one Session watermark.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRuntimeState {
    /// Pure portable Plan/step projection.
    pub snapshot: PlanSnapshot,
    /// Positive contiguous mutable progress version.
    pub state_version: u64,
    /// Current unexpired or started claims keyed by step identity.
    pub active_claims: BTreeMap<PlanStepId, ActivePlanClaim>,
    /// Optimistic-concurrency version of the containing Session.
    pub session_version: u64,
    /// Highest durable position included in reconstruction.
    pub through_position: u64,
}

/// Closed policy posture recorded when one step attempt fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanRetryPosture {
    /// Atomically return the failed step to Ready when bounds allow.
    Retry,
    /// Leave the step failed for an explicit suspension command.
    Suspend,
    /// Leave the step failed while Runtime proposes a replacement revision.
    Replan,
    /// Leave the step failed for explicit Plan terminalization.
    Fail,
}

/// Plan-owned claim and F0 posture used to bind one real C6 Execution start.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanStepExecutionStart {
    /// Claimed step.
    pub step_id: PlanStepId,
    /// Exact claim identity.
    pub claim_id: String,
    /// Exact fencing epoch.
    pub lease_epoch: u64,
    /// Exact monotonic clock revision.
    pub clock_revision: String,
    /// Tick proving start preceded expiry.
    pub observed_at_tick: u64,
    /// Stable attempt identity.
    pub attempt_id: String,
    /// Prepared-v3 Sandbox profile digest frozen for the Execution posture.
    pub sandbox_profile_digest: String,
    /// Fresh Runtime Safety decision identity.
    pub safety_decision_id: String,
}

impl PlanRetryPosture {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Suspend => "suspend",
            Self::Replan => "replan",
            Self::Fail => "fail",
        }
    }
}

/// Runtime metadata for the admitted normal Plan execution path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanRuntimeTransition {
    /// Adopt a proposal under exact Goal and prior-Plan revisions.
    Adopt {
        /// Exact current Goal revision.
        expected_goal_revision: u64,
        /// Prior authoritative Plan revision, when one exists.
        expected_prior_plan_revision: Option<u64>,
        /// Frozen adoption policy reference.
        policy_reference: String,
        /// Canonical verified carry-forward evidence list.
        carry_forward_evidence: CanonicalPayload,
    },
    /// Fence one Ready step before worker dispatch.
    Claim {
        /// Step to claim.
        step_id: PlanStepId,
        /// Stable claim identity.
        claim_id: String,
        /// Worker identity.
        worker_reference: String,
        /// Positive fencing epoch.
        lease_epoch: u64,
        /// Monotonic clock revision.
        clock_revision: String,
        /// Inclusive claim tick.
        claimed_at_tick: u64,
        /// Exclusive expiry tick.
        expires_at_tick: u64,
    },
    /// Return a never-started claim to Ready after proven monotonic expiry.
    ExpireClaim {
        /// Claimed step.
        step_id: PlanStepId,
        /// Exact claim identity.
        claim_id: String,
        /// Exact fencing epoch.
        lease_epoch: u64,
        /// Exact monotonic clock revision.
        clock_revision: String,
        /// Tick proving expiry.
        observed_at_tick: u64,
    },
    /// Start one Kernel Execution only under the current pre-expiry claim.
    Start {
        /// Claimed step.
        step_id: PlanStepId,
        /// Exact claim identity.
        claim_id: String,
        /// Exact fencing epoch.
        lease_epoch: u64,
        /// Exact monotonic clock revision.
        clock_revision: String,
        /// Tick proving start preceded expiry.
        observed_at_tick: u64,
        /// Stable attempt identity.
        attempt_id: String,
        /// Bound C6 Execution identity.
        execution_id: String,
        /// Frozen C6 execution-input snapshot digest.
        execution_snapshot_digest: String,
        /// Prepared-v3 Sandbox profile digest.
        sandbox_profile_digest: String,
        /// Fresh Safety decision identity.
        safety_decision_id: String,
    },
    /// Complete one started attempt with exact evidence bindings.
    CompleteStep {
        /// Running step.
        step_id: PlanStepId,
        /// Exact attempt identity.
        attempt_id: String,
        /// Exact C6 Execution identity.
        execution_id: String,
        /// Stable result digest.
        result_digest: String,
        /// Canonical step evidence list.
        step_evidence: CanonicalPayload,
        /// Canonical Goal-criterion evidence list.
        criterion_evidence: CanonicalPayload,
    },
    /// Fail one started attempt and freeze the admitted follow-up posture.
    FailStep {
        /// Running step.
        step_id: PlanStepId,
        /// Exact attempt identity.
        attempt_id: String,
        /// Exact C6 Execution identity.
        execution_id: String,
        /// Stable safe failure reason.
        reason: String,
        /// Optional canonical failure evidence.
        evidence: Option<CanonicalPayload>,
        /// Closed policy result; Retry is applied in this same transition.
        retry_posture: PlanRetryPosture,
    },
    /// Terminalize only after every step and Goal criterion is verified.
    CompletePlan {
        /// Canonical complete reduction evidence.
        reduction_evidence: CanonicalPayload,
    },
}

/// One validated fact batch and its predicted Plan projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedPlanCommand {
    /// Atomic Ledger batch; currently exactly one Plan command fact.
    pub facts: Vec<FactDraft>,
    /// Projection after applying the planned fact.
    pub next: PlanRuntimeState,
}

/// Stable Runtime failure classes for durable Plan commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanRuntimeError {
    /// Command metadata, binding, or payload is malformed.
    Invalid,
    /// Expected Plan state or Session revision is stale.
    RevisionConflict,
    /// Command identity was reused with different semantics.
    CommandConflict,
    /// Requested lifecycle edge is invalid.
    TransitionInvalid,
    /// Step is not Ready or the claim is not current.
    StepNotReady,
    /// Claim fencing identity, epoch, clock, or expiry is stale.
    ClaimStale,
    /// A hard Plan or step bound was exhausted.
    BoundExceeded,
    /// Bound Goal identity, revision, digest, state, or Session watermark is stale.
    BindingStale,
    /// Persisted Plan facts cannot reconstruct a legal prefix.
    RecoveryCorrupt,
    /// SQLite or Ledger durability failed.
    DurabilityFailure,
}
