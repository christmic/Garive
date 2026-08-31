use std::{collections::BTreeMap, time::Instant};

use garive_host_client::{HostClientErrorCode, LiveHostClient};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    application::{
        reduce, AppAction, AppModel, ConnectionState, EffectKind, ExecutionState, Overlay,
    },
    input::ComposerClickTracker,
    persistence::{now, PendingCommand, PendingKind, Preferences, PromptHistoryEntry, StateStore},
    view, LaunchConfig,
};

use super::{
    super::{
        external_editor::EditorRequest,
        host::{self, HostMessage},
    },
    state_error_name,
};

pub(in crate::runtime) struct RuntimeState {
    pub(in crate::runtime) config: LaunchConfig,
    pub(in crate::runtime) client: LiveHostClient,
    pub(in crate::runtime) sender: mpsc::Sender<HostMessage>,
    pub(in crate::runtime) model: AppModel,
    pub(in crate::runtime) follow: Option<JoinHandle<()>>,
    pub(in crate::runtime) reconnect: Option<JoinHandle<()>>,
    pub(in crate::runtime) reconnect_attempt: u32,
    pub(in crate::runtime) store: StateStore,
    pub(in crate::runtime) preferences: Preferences,
    persisted_preferences: Preferences,
    pub(in crate::runtime) pending: Vec<PendingCommand>,
    pub(in crate::runtime) ephemeral_confirmed: bool,
    pub(in crate::runtime) queued_prompt: Option<String>,
    pub(in crate::runtime) editing_suspension: Option<String>,
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
        if !restored.pending.is_empty() {
            model.notice = Some("A prior command has an unknown durable outcome. Use /retry after reviewing status.".into());
            model.overlay = Some(Overlay::UnknownCommand);
        } else if restored.pending_quarantined != 0 {
            model.notice = Some(format!(
                "Quarantined {} corrupt pending command file(s).",
                restored.pending_quarantined
            ));
        }
        model.has_pending_command = !restored.pending.is_empty();
        model.composer_is_frozen =
            pending_freezes_composer(&restored.pending, model.selected_session.as_deref());
        let persisted_preferences = restored.preferences.clone();
        Self {
            config,
            client,
            sender,
            model,
            follow: None,
            reconnect: None,
            reconnect_attempt: 0,
            store: restored.store,
            preferences: restored.preferences,
            persisted_preferences,
            pending: restored.pending,
            ephemeral_confirmed: false,
            queued_prompt: None,
            editing_suspension: None,
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
            match effect.kind {
                EffectKind::Exit => debug_assert!(self.model.quit_requested),
            }
        }
    }

    pub(in crate::runtime) fn load(&mut self, session_id: String) {
        let switching_session = self.model.selected_session.as_deref() != Some(&session_id);
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

    pub(in crate::runtime) fn persist_presentation(&mut self) {
        if let Some(session) = self.model.selected_session.as_deref() {
            self.preferences
                .set_draft(session, self.model.composer.text());
            self.preferences.selected_session_id = Some(session.into());
        }
        self.preferences.theme = self.config.theme;
        self.preferences.mouse = self.config.mouse;
        self.preferences.reduced_motion = self.config.reduced_motion;
        if let Err(error) = self
            .store
            .save_preferences_merged(&mut self.preferences, &mut self.persisted_preferences)
        {
            self.model.notice = Some(format!("Local state: {}", state_error_name(error)));
        }
    }

    pub(in crate::runtime) fn admit_pending(&mut self, pending: PendingCommand) -> bool {
        if self.store.is_ephemeral() && !self.ephemeral_confirmed {
            self.model.overlay = Some(Overlay::EphemeralConfirmation);
            return false;
        }
        if self
            .pending
            .iter()
            .any(|value| value.session_id == pending.session_id)
        {
            self.model.notice = Some(
                "This Session already has an unknown command outcome. Use /retry first.".into(),
            );
            self.model.overlay = Some(Overlay::UnknownCommand);
            return false;
        }
        let Ok(pending) = pending.seal() else {
            self.local_state_failure("invalid_pending_command");
            return false;
        };
        if self.store.save_pending(&pending).is_err() {
            self.local_state_failure("pending_write_failed");
            return false;
        }
        self.pending.push(pending);
        self.sync_pending_projection();
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingPersisted);
        true
    }

    pub(in crate::runtime) fn pending_for_context(&self) -> Option<&PendingCommand> {
        let selected = self.model.selected_session.as_deref();
        self.pending
            .iter()
            .find(|value| value.session_id.as_deref() == selected)
            .or_else(|| self.pending.first())
    }

    fn sync_pending_projection(&mut self) {
        self.model.has_pending_command = !self.pending.is_empty();
        self.model.composer_is_frozen =
            pending_freezes_composer(&self.pending, self.model.selected_session.as_deref());
    }

    pub(in crate::runtime) fn composer_is_frozen(&self) -> bool {
        self.model.composer_is_frozen
    }

    pub(in crate::runtime) fn explain_frozen_composer(&mut self) {
        self.model.notice =
            Some("This draft is frozen until the pending command reaches durable truth.".into());
    }

    pub(in crate::runtime) fn abandon_pending(&mut self) {
        let Some(command_id) = self
            .pending_for_context()
            .map(|value| value.command_id.clone())
        else {
            self.model.overlay = None;
            return;
        };
        let index = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id)
            .expect("pending command remains present");
        if self
            .store
            .remove_pending(self.pending[index].session_id.as_deref())
            .is_err()
        {
            self.local_state_failure("pending_abandon_failed");
            return;
        }
        self.pending.remove(index);
        self.sync_pending_projection();
        self.model.overlay = None;
        if let Some(session) = self.model.selected_session.clone() {
            self.load(session);
        }
    }

    pub(super) fn finish_pending(&mut self, command_id: &str, submitted_text: &str) {
        let Some(index) = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id)
        else {
            return;
        };
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::ResponseAccepted);
        let pending = self.pending[index].clone();
        if self
            .store
            .remove_pending(pending.session_id.as_deref())
            .is_err()
        {
            self.local_state_failure("pending_remove_failed");
            return;
        }
        #[cfg(feature = "test-hooks")]
        self.crash_if(crate::args::TestCrashHook::PendingRemoved);
        if !self.config.no_prompt_history
            && !submitted_text.is_empty()
            && matches!(
                pending.kind,
                PendingKind::StartTurn | PendingKind::ContinueTurn
            )
        {
            if let Some(session_id) = pending.session_id.clone() {
                let entry = PromptHistoryEntry {
                    schema_version: 1,
                    entry_id: uuid::Uuid::new_v4().to_string(),
                    session_id,
                    submitted_text: submitted_text.into(),
                    submitted_at: now(),
                };
                if self.store.append_history(&entry).is_err() {
                    self.model.notice = Some("Prompt history could not be saved.".into());
                } else {
                    self.model
                        .prompt_history
                        .retain(|value| value != submitted_text);
                    self.model.prompt_history.insert(0, submitted_text.into());
                    self.model.prompt_history.truncate(500);
                }
            }
        }
        self.pending.remove(index);
        self.sync_pending_projection();
    }

    #[cfg(feature = "test-hooks")]
    fn crash_if(&self, point: crate::args::TestCrashHook) {
        if self.config.test_crash_hook == Some(point) {
            let name = match point {
                crate::args::TestCrashHook::TerminalAcquiredPanic => "terminal-acquired-panic",
                crate::args::TestCrashHook::PendingPersisted => "pending-persisted",
                crate::args::TestCrashHook::ResponseAccepted => "response-accepted",
                crate::args::TestCrashHook::PendingRemoved => "pending-removed",
            };
            eprintln!("GARIVE_TEST_CRASH_HOOK={name}");
            loop {
                std::thread::park();
            }
        }
    }

    pub(super) fn reject_pending(&mut self, command_id: &str, code: HostClientErrorCode) {
        let pending_index = self
            .pending
            .iter()
            .position(|value| value.command_id == command_id);
        if matches!(
            code,
            HostClientErrorCode::HostFailure | HostClientErrorCode::InvalidCommand
        ) {
            if let Some(index) = pending_index {
                let pending = self.pending.remove(index);
                let _ = self.store.remove_pending(pending.session_id.as_deref());
            }
            self.sync_pending_projection();
        } else if pending_index.is_some() {
            self.model.notice =
                Some("The command outcome is unknown. Review /status or use exact /retry.".into());
            self.model.overlay = Some(Overlay::UnknownCommand);
        }
    }

    fn local_state_failure(&mut self, code: &str) {
        self.model.notice = Some(format!("Local recovery state: {code}"));
        self.model.overlay = Some(Overlay::ErrorDetails);
    }
}

pub(super) fn pending_freezes_composer(pending: &[PendingCommand], selected: Option<&str>) -> bool {
    pending.iter().any(|pending| {
        pending.session_id.as_deref() == selected
            || (selected.is_none() && pending.kind == PendingKind::CreateSession)
    })
}
