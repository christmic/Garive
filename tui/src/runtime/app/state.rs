use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use garive_host_client::LiveHostClient;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        reduce, AppAction, AppModel, ConnectionState, EffectKind, ExecutionState, Overlay,
    },
    input::ComposerClickTracker,
    persistence::{AsyncStateStore, PendingCommand, Preferences, PromptHistoryEntry, StateStore},
    view, LaunchConfig,
};

use super::super::{
    effects::EffectRunner,
    external_editor::EditorRequest,
    host::{self, HostMessage},
};

mod pending;

pub(super) use pending::pending_freezes_composer;

pub(in crate::runtime) struct RuntimeState {
    pub(in crate::runtime) config: LaunchConfig,
    pub(in crate::runtime) client: LiveHostClient,
    pub(in crate::runtime) sender: mpsc::Sender<HostMessage>,
    effects: EffectRunner<AsyncStateStore>,
    pub(in crate::runtime) model: AppModel,
    pub(in crate::runtime) follow: Option<JoinHandle<()>>,
    pub(in crate::runtime) reconnect: Option<JoinHandle<()>>,
    pub(in crate::runtime) reconnect_attempt: u32,
    pub(in crate::runtime) live_follow: Option<JoinHandle<()>>,
    pub(in crate::runtime) live_reconnect: Option<JoinHandle<()>>,
    pub(in crate::runtime) live_reconnect_attempt: u32,
    pub(in crate::runtime) store: StateStore,
    pub(in crate::runtime) preferences: Preferences,
    persisted_preferences: Preferences,
    pub(in crate::runtime) pending: Vec<PendingCommand>,
    pending_recovery: BTreeSet<String>,
    pub(in crate::runtime) ephemeral_confirmed: bool,
    pub(in crate::runtime) deferred_ephemeral: Option<PendingCommand>,
    pub(in crate::runtime) queued_prompt: Option<String>,
    pub(in crate::runtime) snapshot_request: u64,
    pub(super) background_follows: BTreeMap<String, BackgroundFollow>,
    follow_sequence: u64,
    pub(in crate::runtime) force_redraw: bool,
    pub(in crate::runtime) last_empty_ctrl_c: Option<Instant>,
    pub(in crate::runtime) retry_after_refresh: Option<String>,
    pub(in crate::runtime) render_cache: view::RenderCache,
    pub(in crate::runtime) bell_requested: bool,
    pub(in crate::runtime) composer_mouse_selecting: bool,
    pub(in crate::runtime) composer_clicks: ComposerClickTracker,
    pub(in crate::runtime) external_editor_request: Option<EditorRequest>,
}

pub(super) struct BackgroundFollow {
    pub(super) observed_position: u64,
    pub(super) attempt: u32,
    sequence: u64,
    pub(super) follow: Option<JoinHandle<()>>,
    pub(super) reconnect: Option<JoinHandle<()>>,
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
        restored: RestoredState,
    ) -> Self {
        let mut model = AppModel::default();
        reduce(&mut model, AppAction::BootStarted);
        model.prompt_history = restored
            .history
            .into_iter()
            .rev()
            .map(|entry| entry.submitted_text)
            .collect();
        if restored.history_error {
            model.notice = Some("Corrupt prompt history was quarantined.".into());
        }
        let pending_recovery = restored
            .pending
            .iter()
            .map(|pending| pending.command_id.clone())
            .collect::<BTreeSet<_>>();
        if !pending_recovery.is_empty() {
            model.notice = Some("A prior command has an unknown durable outcome. Use /retry after reviewing status.".into());
            model.overlay = Some(Overlay::UnknownCommand);
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
        let effects =
            EffectRunner::new(AsyncStateStore::new(restored.store.clone()), action_sender);
        Self {
            config,
            client,
            sender,
            effects,
            model,
            follow: None,
            reconnect: None,
            reconnect_attempt: 0,
            live_follow: None,
            live_reconnect: None,
            live_reconnect_attempt: 0,
            store: restored.store,
            preferences: restored.preferences,
            persisted_preferences,
            pending: restored.pending,
            pending_recovery,
            ephemeral_confirmed: false,
            deferred_ephemeral: None,
            queued_prompt: None,
            snapshot_request: 0,
            background_follows: BTreeMap::new(),
            follow_sequence: 0,
            force_redraw: false,
            last_empty_ctrl_c: None,
            retry_after_refresh: None,
            render_cache: view::RenderCache::default(),
            bell_requested: false,
            composer_mouse_selecting: false,
            composer_clicks: ComposerClickTracker::default(),
            external_editor_request: None,
        }
    }

    pub(in crate::runtime) fn command_id(&mut self, _operation: &str) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    pub(in crate::runtime) fn dispatch(&mut self, action: AppAction) {
        for effect in reduce(&mut self.model, action) {
            match effect.kind.tag() {
                crate::application::EffectTag::Exit => {
                    debug_assert!(self.model.quit_requested);
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
            }
        }
    }

    pub(in crate::runtime) fn load(&mut self, session_id: String) {
        self.model.close_turn_navigator();
        let switching_session = self.model.selected_session.as_deref() != Some(&session_id);
        if let Some(task) = self.live_follow.take() {
            task.abort();
        }
        if let Some(task) = self.live_reconnect.take() {
            task.abort();
        }
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
                    self.add_background_follow(previous, self.model.observed_position, task);
                }
            } else if let Some(task) = self.follow.take() {
                task.abort();
            }
        } else if let Some(task) = self.follow.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
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
        self.preferences.selected_session_id = Some(session_id.clone());
        let draft = self
            .preferences
            .draft(&session_id)
            .unwrap_or_default()
            .to_owned();
        let _ = self.model.composer.replace(&draft);
        self.model.prompt_history_browser.reset();
        self.model.connection = ConnectionState::Connecting;
        self.snapshot_request = self.snapshot_request.saturating_add(1);
        host::load_snapshot(
            self.client.clone(),
            self.snapshot_request,
            session_id,
            self.sender.clone(),
        );
    }

    fn add_background_follow(
        &mut self,
        session_id: String,
        observed_position: u64,
        task: JoinHandle<()>,
    ) {
        self.follow_sequence = self.follow_sequence.saturating_add(1);
        if let Some(mut replaced) = self.background_follows.remove(&session_id) {
            if let Some(previous) = replaced.follow.take() {
                previous.abort();
            }
            if let Some(previous) = replaced.reconnect.take() {
                previous.abort();
            }
        }
        self.background_follows.insert(
            session_id,
            BackgroundFollow {
                observed_position,
                attempt: 0,
                sequence: self.follow_sequence,
                follow: Some(task),
                reconnect: None,
            },
        );
        if self.background_follows.len() > 4 {
            let oldest = self
                .background_follows
                .iter()
                .min_by_key(|(_, value)| value.sequence)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                if let Some(mut evicted) = self.background_follows.remove(&oldest) {
                    if let Some(task) = evicted.follow.take() {
                        task.abort();
                    }
                    if let Some(task) = evicted.reconnect.take() {
                        task.abort();
                    }
                }
            }
        }
    }

    pub(in crate::runtime) fn stop_tasks(&mut self) {
        if let Some(task) = self.follow.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect.take() {
            task.abort();
        }
        if let Some(task) = self.live_follow.take() {
            task.abort();
        }
        if let Some(task) = self.live_reconnect.take() {
            task.abort();
        }
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
        if self.model.sessions_loading {
            return;
        }
        let Some(before) = self.model.sessions_next_before.clone() else {
            return;
        };
        self.model.sessions_loading = true;
        host::load_session_page(self.client.clone(), before, self.sender.clone());
    }
}
