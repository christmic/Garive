use std::collections::BTreeMap;

use serde::Serialize;

use crate::{PlanDefinitionV1, PlanError, PlanErrorCode, PlanStepId};

/// Closed lifecycle for one immutable Plan revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    /// Valid proposal awaiting Runtime authority.
    Proposed,
    /// Authoritative revision with no started step yet.
    Adopted,
    /// At least one step has started or is available for execution.
    Running,
    /// Plan-level continuation is required.
    Suspended,
    /// Every step and Goal-criterion reduction was verified.
    Completed,
    /// Explicit terminal failure.
    Failed,
    /// A newer revision atomically replaced this revision.
    Superseded,
    /// Runtime authority rejected this proposal.
    Rejected,
}

impl PlanState {
    const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Superseded | Self::Rejected
        )
    }
}

/// Closed progress state for one declared step.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    /// Dependencies are not yet complete.
    Pending,
    /// Dependencies and hard bounds currently admit a claim.
    Ready,
    /// A fenced Runtime claim exists but no attempt started.
    Claimed,
    /// One attempt owns a Kernel Execution.
    Running,
    /// Typed continuation or reconciliation is required.
    Suspended,
    /// Verified terminal evidence exists.
    Completed,
    /// The last attempt failed; policy may admit a retry.
    Failed,
}

/// Immutable public progress for one step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepProgress {
    state: StepState,
    attempts: u32,
}

impl StepProgress {
    /// Returns current pure step state.
    pub const fn state(self) -> StepState {
        self.state
    }

    /// Returns the number of started attempts.
    pub const fn attempts(self) -> u32 {
        self.attempts
    }
}

/// One requested pure Plan/step transition after Runtime command validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanTransition {
    /// Adopt a valid proposal.
    Adopt,
    /// Reject a proposal.
    Reject,
    /// Suspend Plan-level dispatch.
    Suspend,
    /// Resume Plan-level dispatch.
    Resume,
    /// Supersede with a separately validated revision.
    Supersede,
    /// Explicitly terminalize as failed.
    Fail,
    /// Complete only when every step and criterion is verified.
    Complete {
        /// Whether Runtime verified complete Goal-criterion evidence.
        criteria_complete: bool,
    },
    /// Claim one Ready step.
    Claim(PlanStepId),
    /// Expire a never-started fenced claim.
    ExpireClaim(PlanStepId),
    /// Start one claimed attempt.
    Start(PlanStepId),
    /// Complete one running attempt.
    CompleteStep(PlanStepId),
    /// Suspend one running attempt.
    SuspendStep(PlanStepId),
    /// Resume a step after its continuation is durably resolved.
    ResumeStep(PlanStepId),
    /// Fail one running attempt.
    FailStep(PlanStepId),
    /// Admit a bounded retry for a failed step.
    RetryStep(PlanStepId),
}

/// Immutable Plan projection after one contiguous transition prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanSnapshot {
    definition: PlanDefinitionV1,
    state: PlanState,
    steps: BTreeMap<PlanStepId, StepProgress>,
    total_attempts: u32,
}

impl PlanSnapshot {
    /// Creates a Proposed projection with every step Pending.
    pub fn new(definition: PlanDefinitionV1) -> Self {
        let steps = definition
            .steps()
            .iter()
            .map(|step| {
                (
                    step.step_id().clone(),
                    StepProgress {
                        state: StepState::Pending,
                        attempts: 0,
                    },
                )
            })
            .collect();
        Self {
            definition,
            state: PlanState::Proposed,
            steps,
            total_attempts: 0,
        }
    }

    /// Returns current Plan state.
    pub const fn state(&self) -> PlanState {
        self.state
    }

    /// Returns total started attempts across all steps.
    pub const fn total_attempts(&self) -> u32 {
        self.total_attempts
    }

    /// Returns progress for one declared step.
    pub fn step(&self, step_id: &PlanStepId) -> Option<StepProgress> {
        self.steps.get(step_id).copied()
    }

    /// Returns Ready steps in semantic declaration order.
    pub fn ready_steps(&self) -> Vec<&PlanStepId> {
        self.definition
            .steps()
            .iter()
            .filter_map(|step| {
                (self.steps[step.step_id()].state == StepState::Ready).then_some(step.step_id())
            })
            .collect()
    }

    /// Applies one legal transition without allocating Runtime identities.
    pub fn apply(&self, transition: PlanTransition) -> Result<Self, PlanError> {
        if self.state.is_terminal() {
            return Err(PlanError::new(PlanErrorCode::PlanTransitionInvalid));
        }
        let mut next = self.clone();
        match transition {
            PlanTransition::Adopt if self.state == PlanState::Proposed => {
                next.state = PlanState::Adopted;
                next.refresh_ready();
            }
            PlanTransition::Reject if self.state == PlanState::Proposed => {
                next.state = PlanState::Rejected;
            }
            PlanTransition::Suspend if self.state == PlanState::Running => {
                next.state = PlanState::Suspended;
            }
            PlanTransition::Resume if self.state == PlanState::Suspended => {
                next.state = PlanState::Running;
                next.refresh_ready();
            }
            PlanTransition::Supersede
                if matches!(
                    self.state,
                    PlanState::Adopted | PlanState::Running | PlanState::Suspended
                ) =>
            {
                next.state = PlanState::Superseded;
            }
            PlanTransition::Fail
                if matches!(self.state, PlanState::Running | PlanState::Suspended) =>
            {
                next.state = PlanState::Failed;
            }
            PlanTransition::Complete { criteria_complete }
                if self.state == PlanState::Running
                    && criteria_complete
                    && self
                        .steps
                        .values()
                        .all(|step| step.state == StepState::Completed) =>
            {
                next.state = PlanState::Completed;
            }
            PlanTransition::Claim(step_id) => next.claim(&step_id)?,
            PlanTransition::ExpireClaim(step_id) => {
                next.require_state(&step_id, StepState::Claimed)?.state = StepState::Ready;
            }
            PlanTransition::Start(step_id) => next.start(&step_id)?,
            PlanTransition::CompleteStep(step_id) => {
                next.require_state(&step_id, StepState::Running)?.state = StepState::Completed;
                next.refresh_ready();
            }
            PlanTransition::SuspendStep(step_id) => {
                next.require_state(&step_id, StepState::Running)?.state = StepState::Suspended;
            }
            PlanTransition::ResumeStep(step_id) => {
                next.require_state(&step_id, StepState::Suspended)?.state = StepState::Pending;
                next.refresh_ready();
            }
            PlanTransition::FailStep(step_id) => {
                next.require_state(&step_id, StepState::Running)?.state = StepState::Failed;
            }
            PlanTransition::RetryStep(step_id) => {
                next.require_state(&step_id, StepState::Failed)?.state = StepState::Pending;
                next.refresh_ready();
            }
            _ => return Err(PlanError::new(PlanErrorCode::PlanTransitionInvalid)),
        }
        Ok(next)
    }

    fn claim(&mut self, step_id: &PlanStepId) -> Result<(), PlanError> {
        if !matches!(self.state, PlanState::Adopted | PlanState::Running)
            || self.active_count() >= self.definition.bounds().max_parallel_ready()
        {
            return Err(PlanError::new(PlanErrorCode::StepNotReady));
        }
        self.require_state(step_id, StepState::Ready)?.state = StepState::Claimed;
        Ok(())
    }

    fn start(&mut self, step_id: &PlanStepId) -> Result<(), PlanError> {
        let step_limit = self
            .definition
            .steps()
            .iter()
            .find(|step| step.step_id() == step_id)
            .ok_or_else(|| PlanError::new(PlanErrorCode::PlanInvalid))?
            .max_attempts();
        let attempts = self
            .steps
            .get(step_id)
            .filter(|progress| progress.state == StepState::Claimed)
            .ok_or_else(|| PlanError::new(PlanErrorCode::PlanTransitionInvalid))?
            .attempts;
        if attempts >= step_limit
            || self.total_attempts >= self.definition.bounds().max_total_attempts()
        {
            return Err(PlanError::new(PlanErrorCode::PlanBoundExceeded));
        }
        let progress = self.require_state(step_id, StepState::Claimed)?;
        progress.attempts += 1;
        progress.state = StepState::Running;
        self.total_attempts += 1;
        self.state = PlanState::Running;
        Ok(())
    }

    fn require_state(
        &mut self,
        step_id: &PlanStepId,
        state: StepState,
    ) -> Result<&mut StepProgress, PlanError> {
        self.steps
            .get_mut(step_id)
            .filter(|progress| progress.state == state)
            .ok_or_else(|| PlanError::new(PlanErrorCode::PlanTransitionInvalid))
    }

    fn active_count(&self) -> u32 {
        self.steps
            .values()
            .filter(|step| matches!(step.state, StepState::Claimed | StepState::Running))
            .count() as u32
    }

    fn refresh_ready(&mut self) {
        if !matches!(self.state, PlanState::Adopted | PlanState::Running)
            || self.total_attempts >= self.definition.bounds().max_total_attempts()
        {
            return;
        }
        let completed: std::collections::BTreeSet<_> = self
            .steps
            .iter()
            .filter_map(|(id, progress)| (progress.state == StepState::Completed).then_some(id))
            .cloned()
            .collect();
        for step in self.definition.steps() {
            let progress = self.steps.get_mut(step.step_id()).expect("validated step");
            if progress.state == StepState::Pending
                && progress.attempts < step.max_attempts()
                && step.depends_on().is_subset(&completed)
            {
                progress.state = StepState::Ready;
            }
        }
    }
}
