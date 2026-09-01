use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use garive_host_client::LiveHostClient;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        reduce, AppAction, AppModel, ConnectionState, EffectKind, ExecutionState, Overlay,
        SessionPagePurpose, SessionPageRequest, SnapshotRequest,
    },
    host::LiveHostReadPort,
    input::ComposerClickTracker,
    persistence::{AsyncStateStore, PendingCommand, Preferences, PromptHistoryEntry, StateStore},
    view, LaunchConfig,
};

use super::super::{
    effects::EffectRunner,
    external_editor::EditorRequest,
    host::{self, HostMessage, LiveSubscriptionId, SubscriptionId},
    host_effects::HostEffectRunner,
    TerminalReconfiguration, TerminalTheme,
};

mod mutations;
mod pending;
mod retry;
mod shutdown;
mod subscriptions;
#[cfg(test)]
mod test_support;

#[cfg(test)]
pub(super) use pending::pending_command_projection;
pub(super) use pending::pending_freezes_composer;

pub(in crate::runtime) struct RuntimeState {
    pub(in crate::runtime) config: LaunchConfig,
    pub(in crate::runtime) client: LiveHostClient,
    pub(in crate::runtime) sender: mpsc::Sender<HostMessage>,
    effects: EffectRunner<AsyncStateStore>,
    host_effects: HostEffectRunner<LiveHostReadPort>,
    pub(in crate::runtime) model: AppModel,
    pub(in crate::runtime) follow: Option<JoinHandle<()>>,
    pub(in crate::runtime) follow_owner: Option<SubscriptionId>,
    pub(in crate::runtime) reconnect: Option<JoinHandle<()>>,
    pub(in crate::runtime) reconnect_owner: Option<SubscriptionId>,
    pub(in crate::runtime) reconnect_attempt: u32,
    pub(in crate::runtime) live_follow: Option<JoinHandle<()>>,
    pub(in crate::runtime) live_follow_owner: Option<LiveSubscriptionId>,
    pub(in crate::runtime) live_reconnect: Option<JoinHandle<()>>,
    pub(in crate::runtime) live_reconnect_owner: Option<LiveSubscriptionId>,
    pub(in crate::runtime) live_reconnect_attempt: u32,
    pub(in crate::runtime) store: StateStore,
    pub(in crate::runtime) preferences: Preferences,
    persisted_preferences: Preferences,
    pub(in crate::runtime) pending: Vec<PendingCommand>,
    pending_recovery: BTreeSet<String>,
    pub(in crate::runtime) ephemeral_confirmed: bool,
    pub(in crate::runtime) deferred_ephemeral: Option<PendingCommand>,
    pub(in crate::runtime) deferred_continuation_schema_digest: Option<String>,
    pub(in crate::runtime) queued_prompt: Option<String>,
    graceful_quit_armed: bool,
    pub(super) background_follows: BTreeMap<String, BackgroundFollow>,
    follow_sequence: u64,
    subscription_sequence: u64,
    live_subscription_sequence: u64,
    pub(in crate::runtime) force_redraw: bool,
    pub(in crate::runtime) last_empty_ctrl_c: Option<Instant>,
    exact_retry_owner: Option<ExactRetryOwner>,
    pub(in crate::runtime) render_cache: view::RenderCache,
    pub(in crate::runtime) bell_requested: bool,
    pub(in crate::runtime) composer_mouse_selecting: bool,
    pub(in crate::runtime) composer_clicks: ComposerClickTracker,
    pub(in crate::runtime) external_editor_request: Option<EditorRequest>,
    terminal_reconfiguration: Option<TerminalReconfiguration>,
    terminal_theme: TerminalTheme,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactRetryPhase {
    Refreshing,
    Replayed,
}

struct ExactRetryOwner {
    command_id: String,
    phase: ExactRetryPhase,
}

pub(super) struct BackgroundFollow {
    pub(super) observed_position: u64,
    pub(super) attempt: u32,
    sequence: u64,
    pub(super) follow: Option<JoinHandle<()>>,
    pub(super) follow_owner: Option<SubscriptionId>,
    pub(super) reconnect: Option<JoinHandle<()>>,
    pub(super) reconnect_owner: Option<SubscriptionId>,
}

pub(super) struct RestoredState {
    pub(super) store: StateStore,
    pub(super) preferences: Preferences,
    pub(super) pending: Vec<PendingCommand>,
    pub(super) pending_quarantined: usize,
    pub(super) history: Vec<PromptHistoryEntry>,
    pub(super) history_error: bool,
}

impl RuntimeState {
    pub(super) fn new(
        config: LaunchConfig,
        client: LiveHostClient,
        sender: mpsc::Sender<HostMessage>,
        action_sender: mpsc::Sender<AppAction>,
        terminal_theme: TerminalTheme,
        restored: RestoredState,
    ) -> Self {
        let mut model = AppModel {
            prompt_history: restored
                .history
                .into_iter()
                .rev()
                .map(|entry| entry.submitted_text)
                .collect(),
            ..Default::default()
        };
        if restored.history_error {
            model.notice = Some("Corrupt prompt history was quarantined.".into());
        }
        let pending_recovery = restored
            .pending
            .iter()
            .map(|pending| pending.command_id.clone())
            .collect::<BTreeSet<_>>();
        for pending in &restored.pending {
            if pending.kind == crate::persistence::PendingKind::CancelTurn {
                if let (Some(session_id), Some(turn_id)) =
                    (pending.session_id.clone(), pending.turn_id.clone())
                {
                    model
                        .cancel_requests
                        .begin(pending.command_id.clone(), session_id, turn_id);
                    model.cancel_requests.mark_unknown(&pending.command_id);
                }
            }
        }
        if !pending_recovery.is_empty() {
            model.notice = Some("A prior command has an unknown durable outcome. Use /retry after reviewing status.".into());
            if restored.pending.iter().any(|pending| {
                pending.session_id.is_none() && pending_recovery.contains(&pending.command_id)
            }) {
                model.overlay = Some(Overlay::UnknownCommand);
            }
        } else if restored.pending_quarantined != 0 {
            model.notice = Some(format!(
                "Quarantined {} corrupt pending command file(s).",
                restored.pending_quarantined
            ));
        }
        model.has_pending_command = !restored.pending.is_empty();
        model.pending_recovery.current_session = restored.pending.iter().any(|pending| {
            pending_recovery.contains(&pending.command_id) && pending.session_id.is_none()
        });
        model.pending_recovery.other_session = restored.pending.iter().any(|pending| {
            pending_recovery.contains(&pending.command_id) && pending.session_id.is_some()
        });
        model.composer_is_frozen =
            pending_freezes_composer(&restored.pending, model.selected_session.as_deref());
        model.inspector.open = restored.preferences.activity_inspector;
        let persisted_preferences = restored.preferences.clone();
        let host_effects =
            HostEffectRunner::new(LiveHostReadPort::new(client.clone()), action_sender.clone());
        let effects =
            EffectRunner::new(AsyncStateStore::new(restored.store.clone()), action_sender);
        Self {
            config,
            client,
            sender,
            effects,
            host_effects,
            model,
            follow: None,
            follow_owner: None,
            reconnect: None,
            reconnect_owner: None,
            reconnect_attempt: 0,
            live_follow: None,
            live_follow_owner: None,
            live_reconnect: None,
            live_reconnect_owner: None,
            live_reconnect_attempt: 0,
            store: restored.store,
            preferences: restored.preferences,
            persisted_preferences,
            pending: restored.pending,
            pending_recovery,
            ephemeral_confirmed: false,
            deferred_ephemeral: None,
            deferred_continuation_schema_digest: None,
            queued_prompt: None,
            graceful_quit_armed: false,
            background_follows: BTreeMap::new(),
            follow_sequence: 0,
            subscription_sequence: 0,
            live_subscription_sequence: 0,
            force_redraw: false,
            last_empty_ctrl_c: None,
            exact_retry_owner: None,
            render_cache: view::RenderCache::default(),
            bell_requested: false,
            composer_mouse_selecting: false,
            composer_clicks: ComposerClickTracker::default(),
            external_editor_request: None,
            terminal_reconfiguration: None,
            terminal_theme,
        }
    }

    pub(in crate::runtime) fn theme(&self) -> crate::Theme {
        self.terminal_theme.resolve(self.config.theme)
    }

    pub(in crate::runtime) fn set_mouse_mode(&mut self, mode: crate::MouseMode) {
        self.config.mouse = mode;
        let enabled = crate::args::mouse_capture_enabled(mode, self.config.screen_reader);
        if !self.config.screen_reader {
            self.terminal_reconfiguration = Some(TerminalReconfiguration::MouseCapture { enabled });
        }
        self.model.notice = Some(
            match (self.config.screen_reader, mode, enabled) {
                (true, _, _) => "Mouse capture stays disabled in accessible terminal mode.",
                (false, crate::MouseMode::Auto, _) => {
                    "Garive mouse capture is off; terminal selection and scrolling remain available."
                }
                (false, crate::MouseMode::On, _) => {
                    "Mouse capture is enabled for this terminal session."
                }
                (false, crate::MouseMode::Off, _) => {
                    "Mouse capture is disabled for this terminal session."
                }
            }
            .into(),
        );
        self.model.overlay = Some(Overlay::ErrorDetails);
    }

    pub(in crate::runtime) fn take_terminal_reconfiguration(
        &mut self,
    ) -> Option<TerminalReconfiguration> {
        self.terminal_reconfiguration.take()
    }

    pub(in crate::runtime) fn command_id(&mut self, _operation: &str) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    pub(in crate::runtime) fn dispatch(&mut self, action: AppAction) {
        let boot_revision = self.model.boot_completion_revision;
        let catalog_revision = self.model.catalog_refresh_revision;
        let snapshot_revision = self.model.snapshot_completion_revision;
        let effects = reduce(&mut self.model, action);
        self.sync_pending_projection();
        for effect in effects {
            match effect.kind.tag() {
                crate::application::EffectTag::Exit => {
                    debug_assert!(self.model.quit_requested);
                }
                crate::application::EffectTag::LoadDefinitions
                | crate::application::EffectTag::LoadSessionPage
                | crate::application::EffectTag::LoadSnapshot => {
                    self.host_effects.submit(effect);
                }
                crate::application::EffectTag::PersistPending => self.effects.submit(effect),
                crate::application::EffectTag::StartTurn => {
                    let EffectKind::StartTurn { draft, identity } = effect.kind else {
                        unreachable!("effect tag and payload agree");
                    };
                    if let Some((command_id, session_id, text)) =
                        self.activate_persisted_start(draft, identity)
                    {
                        host::start_turn(
                            self.client.clone(),
                            command_id,
                            session_id,
                            text,
                            self.sender.clone(),
                        );
                    }
                }
                crate::application::EffectTag::CreateSession => {
                    let EffectKind::CreateSession { draft, identity } = effect.kind else {
                        unreachable!("effect tag and payload agree");
                    };
                    if let Some((command_id, definition_id)) =
                        self.activate_persisted_create(draft, identity)
                    {
                        host::create_session(
                            self.client.clone(),
                            command_id,
                            definition_id,
                            self.sender.clone(),
                        );
                    }
                }
                crate::application::EffectTag::CancelTurn => {
                    let EffectKind::CancelTurn { draft, identity } = effect.kind else {
                        unreachable!("effect tag and payload agree");
                    };
                    if let Some((command_id, session_id, turn_id, position)) =
                        self.activate_persisted_cancel(draft, identity)
                    {
                        host::cancel_turn(
                            self.client.clone(),
                            command_id,
                            session_id,
                            turn_id,
                            position,
                            self.sender.clone(),
                        );
                    }
                }
                crate::application::EffectTag::ContinueTurn => {
                    let EffectKind::ContinueTurn {
                        draft,
                        identity,
                        host_allowed,
                        ..
                    } = effect.kind
                    else {
                        unreachable!("effect tag and payload agree");
                    };
                    if let Some(request) =
                        self.activate_persisted_continue(draft, identity, host_allowed)
                    {
                        host::continue_turn(self.client.clone(), request, self.sender.clone());
                    }
                }
                crate::application::EffectTag::PersistContinuation => self.effects.submit(effect),
            }
        }
        self.sync_pending_projection();
        if self.model.boot_completion_revision != boot_revision {
            super::messages::apply_boot_completion(self);
        }
        if self.model.catalog_refresh_revision != catalog_revision {
            super::messages::apply_catalog_refresh_completion(self);
        }
        if self.model.snapshot_completion_revision != snapshot_revision {
            super::messages::apply_snapshot_completion(self);
        }
    }

    pub(in crate::runtime) fn load(&mut self, session_id: String) {
        self.model.close_turn_navigator();
        let switching_session = self.model.selected_session.as_deref() != Some(&session_id);
        if let Some(task) = self.live_follow.take() {
            task.abort();
        }
        self.live_follow_owner = None;
        if let Some(task) = self.live_reconnect.take() {
            task.abort();
        }
        self.live_reconnect_owner = None;
        self.live_reconnect_attempt = 0;
        if switching_session {
            self.model.live_answer.clear_for_session_change();
            self.model.active_execution_id = None;
        }
        if switching_session {
            if matches!(
                self.model.execution,
                ExecutionState::Following | ExecutionState::Suspended
            ) {
                if let (Some(previous), Some(task)) =
                    (self.model.selected_session.clone(), self.follow.take())
                {
                    if let Some(owner) = self.follow_owner.take() {
                        self.add_background_follow(
                            previous,
                            self.model.observed_position,
                            owner,
                            task,
                        );
                    } else {
                        task.abort();
                    }
                }
            } else if let Some(task) = self.follow.take() {
                task.abort();
                self.follow_owner = None;
            }
        } else if let Some(task) = self.follow.take() {
            task.abort();
            self.follow_owner = None;
        }
        self.follow_owner = None;
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
        self.reconnect_owner = None;
        self.reconnect_attempt = 0;
        if let Some(mut background) = self.background_follows.remove(&session_id) {
            if let Some(task) = background.follow.take() {
                task.abort();
            }
            if let Some(task) = background.reconnect.take() {
                task.abort();
            }
        }
        self.persist_presentation();
        if switching_session {
            self.model.switch_viewport(&session_id);
            self.model.composer.clear_private_edit_buffer();
        }
        self.model.selected_session = Some(session_id.clone());
        self.sync_pending_projection();
        if switching_session
            && !self.exact_retry_in_progress()
            && self.recoverable_pending_for_context().is_some()
        {
            self.model.notice = Some(
                "A prior command has an unknown durable outcome. Use /retry after reviewing status."
                    .into(),
            );
            self.model.overlay = Some(Overlay::UnknownCommand);
        }
        self.preferences.selected_session_id = Some(session_id.clone());
        let draft = self
            .preferences
            .draft(&session_id)
            .unwrap_or_default()
            .to_owned();
        let _ = self.model.composer.replace(&draft);
        self.model.prompt_history_browser.reset();
        self.model.connection = ConnectionState::Connecting;
        self.dispatch(AppAction::LoadSnapshotRequested(SnapshotRequest {
            session_id,
        }));
    }

    pub(in crate::runtime) fn stop_tasks(&mut self) {
        if let Some(task) = self.follow.take() {
            task.abort();
        }
        self.follow_owner = None;
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
        self.reconnect_owner = None;
        if let Some(task) = self.live_follow.take() {
            task.abort();
        }
        self.live_follow_owner = None;
        if let Some(task) = self.live_reconnect.take() {
            task.abort();
        }
        self.live_reconnect_owner = None;
        for background in self.background_follows.values_mut() {
            if let Some(task) = background.follow.take() {
                task.abort();
            }
            if let Some(task) = background.reconnect.take() {
                task.abort();
            }
        }
    }

    pub(in crate::runtime) fn load_more_sessions(&mut self) {
        let Some(before) = self.model.sessions_next_before.clone() else {
            return;
        };
        self.dispatch(AppAction::LoadSessionPageRequested(SessionPageRequest {
            cursor: Some(before),
            purpose: SessionPagePurpose::Append,
        }));
    }

    pub(in crate::runtime) fn refresh_session_catalog(&mut self) {
        self.dispatch(AppAction::LoadSessionPageRequested(SessionPageRequest {
            cursor: None,
            purpose: SessionPagePurpose::CatalogRefresh,
        }));
    }
}
