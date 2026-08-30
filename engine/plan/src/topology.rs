use std::collections::{BTreeMap, BTreeSet};

use crate::{PlanError, PlanErrorCode, PlanStepId, PlanStepV1};

pub(super) fn validate_acyclic(steps: &[PlanStepV1]) -> Result<(), PlanError> {
    let mut remaining: BTreeMap<_, _> = steps
        .iter()
        .map(|step| (step.step_id().clone(), step.depends_on().clone()))
        .collect();
    let mut completed = BTreeSet::new();
    loop {
        let ready: Vec<PlanStepId> = steps
            .iter()
            .filter(|step| {
                remaining
                    .get(step.step_id())
                    .is_some_and(|dependencies| dependencies.is_subset(&completed))
            })
            .map(|step| step.step_id().clone())
            .collect();
        if ready.is_empty() {
            break;
        }
        for step_id in ready {
            remaining.remove(&step_id);
            completed.insert(step_id);
        }
    }
    if remaining.is_empty() {
        Ok(())
    } else {
        Err(PlanError::new(PlanErrorCode::PlanCycle))
    }
}
