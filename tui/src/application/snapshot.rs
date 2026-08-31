use garive_host_client::HostClientErrorCode;

use super::{
    AppEffect, AppEffectOutcome, AppModel, BootState, ConnectionState, EffectContext,
    ExecutionState, HostReadResponse, SnapshotRequest,
};

pub(super) fn finish(
    model: &mut AppModel,
    context: EffectContext,
    request: SnapshotRequest,
    outcome: AppEffectOutcome,
) -> Vec<AppEffect> {
    let is_active = model.selected_session.as_deref() == Some(request.session_id.as_str())
        && model
            .snapshot_owner
            .as_ref()
            .is_some_and(|owner| owner.context == context && owner.request == request);
    if !is_active {
        return Vec::new();
    }
    model.snapshot_owner = None;
    model.snapshot_handoff = None;
    model.snapshot_failure = None;
    match outcome {
        AppEffectOutcome::HostRead(Ok(HostReadResponse::Snapshot(snapshot)))
            if snapshot.request == request =>
        {
            model.snapshot_handoff = Some(*snapshot);
        }
        AppEffectOutcome::HostRead(Err(failure)) => apply_failure(model, failure),
        _ => model.notice = Some("Ignored an invalid Snapshot response.".into()),
    }
    model.snapshot_completion_revision = model.snapshot_completion_revision.saturating_add(1);
    Vec::new()
}

fn apply_failure(model: &mut AppModel, failure: super::HostReadFailure) {
    model.snapshot_failure = Some(failure);
    if matches!(
        failure.code,
        HostClientErrorCode::InvalidConfiguration
            | HostClientErrorCode::InvalidEvent
            | HostClientErrorCode::EventOrderViolation
            | HostClientErrorCode::EventLimitExceeded
    ) {
        model.boot = BootState::Degraded;
        model.connection = ConnectionState::Unavailable {
            safe_code: failure.code.wire_name(),
        };
        model.execution = ExecutionState::Failed;
    } else if failure.host_rejected {
        model.connection = ConnectionState::Online;
    } else {
        model.connection = ConnectionState::Disconnected { attempt: 0 };
    }
    model.notice = Some(format!(
        "Snapshot unavailable: {}.",
        failure.code.wire_name()
    ));
}
