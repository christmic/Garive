use super::{
    AppEffect, AppEffectOutcome, AppModel, BootPartState, BootState, ConnectionState,
    EffectContext, EffectKind, HostReadResponse, SessionPageOwner, SessionPagePurpose,
    SessionPageRequest,
};

pub(super) fn begin(model: &mut AppModel) -> Vec<AppEffect> {
    model.boot = BootState::Loading;
    model.connection = ConnectionState::Connecting;
    let mut effects = Vec::with_capacity(2);
    if let Some(effect) = model.effects.issue(EffectKind::LoadDefinitions, None, None) {
        model.boot_definitions = BootPartState::Loading(effect.context.clone());
        effects.push(effect);
    } else {
        model.boot_definitions = failed("internal_failure");
    }
    let request = SessionPageRequest {
        cursor: None,
        purpose: SessionPagePurpose::Replace,
    };
    if let Some(effect) = model.effects.issue(
        EffectKind::LoadSessionPage {
            request: request.clone(),
        },
        None,
        Some(request.identity_digest()),
    ) {
        model.boot_sessions = BootPartState::Loading(effect.context.clone());
        model.session_page_owner = Some(SessionPageOwner {
            context: effect.context.clone(),
            request,
        });
        model.sessions_loading = true;
        effects.push(effect);
    } else {
        model.boot_sessions = failed("internal_failure");
    }
    settle(model);
    effects
}

pub(super) fn finish_definitions(
    model: &mut AppModel,
    context: EffectContext,
    outcome: AppEffectOutcome,
) {
    if !matches!(&model.boot_definitions, BootPartState::Loading(owner) if owner == &context) {
        return;
    }
    model.boot_definitions = match outcome {
        AppEffectOutcome::HostRead(Ok(HostReadResponse::Definitions(definitions))) => {
            model.definitions = definitions;
            model.definition_count = model.definitions.len();
            BootPartState::Ready
        }
        AppEffectOutcome::HostRead(Err(failure)) => failed(failure.code.wire_name()),
        _ => failed("internal_failure"),
    };
    settle(model);
}

pub(super) fn finish_sessions(model: &mut AppModel, result: Result<(), &'static str>) {
    model.boot_sessions = match result {
        Ok(()) => BootPartState::Ready,
        Err(safe_code) => failed(safe_code),
    };
    settle(model);
}

fn settle(model: &mut AppModel) {
    if model.boot != BootState::Loading
        || matches!(
            model.boot_definitions,
            BootPartState::Idle | BootPartState::Loading(_)
        )
        || matches!(
            model.boot_sessions,
            BootPartState::Idle | BootPartState::Loading(_)
        )
    {
        return;
    }
    model.boot_completion_revision = model.boot_completion_revision.saturating_add(1);
    let failure = [&model.boot_definitions, &model.boot_sessions]
        .into_iter()
        .find_map(|part| match part {
            BootPartState::Failed { safe_code } => Some(*safe_code),
            _ => None,
        });
    if let Some(safe_code) = failure {
        model.boot = BootState::Degraded;
        model.connection = ConnectionState::Unavailable { safe_code };
    } else {
        model.boot = if model.definition_count == 0 {
            BootState::NotConfigured
        } else {
            BootState::Ready
        };
        model.connection = ConnectionState::Online;
    }
}

const fn failed(safe_code: &'static str) -> BootPartState {
    BootPartState::Failed { safe_code }
}
