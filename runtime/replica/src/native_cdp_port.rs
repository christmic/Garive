//! Concrete Runtime-owned T2 port for one explicitly attached CDP page.

use std::{collections::VecDeque, fmt, time::Duration};

use garive_browser_cdp::{
    CdpAxProperty, CdpAxTree, CdpClient, CdpFrameTree, CdpHistoryEntry, CdpNavigationOutcome,
    CdpNavigationResult, CdpPortableKey, CdpSelectOutcome, CdpTransportError, CdpWaitUntil,
    CDP_ADAPTER_REVISION,
};
use garive_tools::canonical_http_origin;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    map_cdp_ax_tree_with_frame_scope, CdpElementTarget, CdpFrameScope, CdpObservationContext,
    CdpSnapshotBindingV1, NativeActionCommandV1, NativeActionFuture, NativeActionReceiptV1,
    NativeAdapterBindingV1, NativeAdapterPort, NativeNodeRef, NativeObservationBounds,
    NativeObservationFuture, NativeProtocolError, NativeSnapshotId, NativeTarget,
};

const ADAPTER_ID: &str = "garive.browser.cdp";
const MAX_PENDING_POPUPS: usize = 32;
const MAX_POPUPS_PER_ACTION: usize = 8;
const POPUP_ADMISSION_ATTEMPTS: usize = 50;
const POPUP_ADMISSION_INTERVAL: Duration = Duration::from_millis(10);

/// Explicit Browser session posture controlling target discovery boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdpBrowserSessionMode {
    /// Dedicated Garive profile; opener-filtered target discovery is admissible.
    Managed,
    /// User-attached page; global target discovery is forbidden.
    Attached,
}

/// One newly created page awaiting a separate Runtime admission decision.
#[derive(Clone, Eq, PartialEq)]
pub struct CdpPendingPopup {
    /// Runtime-owned opaque admission identity; never a CDP target identity.
    pub admission_id: CdpPopupAdmissionId,
    /// Canonical requested origin proven at creation time.
    pub requested_origin: String,
    /// Whether Chromium attributed creation to a user gesture.
    pub user_gesture: bool,
    popup: garive_browser_cdp::CdpPopup,
}

impl fmt::Debug for CdpPendingPopup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CdpPendingPopup")
            .field("admission_id", &self.admission_id)
            .field("requested_origin", &self.requested_origin)
            .field("user_gesture", &self.user_gesture)
            .finish_non_exhaustive()
    }
}

/// Runtime-owned opaque identity for one pending popup admission decision.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CdpPopupAdmissionId(String);

impl CdpPopupAdmissionId {
    /// Returns the opaque identity without exposing protocol target identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Exact private protocol binding for one Runtime-owned Browser page identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpPageBinding {
    target: NativeTarget,
    cdp_target_id: String,
    cdp_session_id: String,
}

impl CdpPageBinding {
    /// Binds one opaque Runtime page to exact private CDP target/session identities.
    pub fn new(
        target: NativeTarget,
        cdp_target_id: impl Into<String>,
        cdp_session_id: impl Into<String>,
    ) -> Result<Self, NativeProtocolError> {
        let cdp_target_id = cdp_target_id.into();
        let cdp_session_id = cdp_session_id.into();
        if !matches!(target, NativeTarget::Browser { .. })
            || !bounded_protocol_id(&cdp_target_id)
            || !bounded_protocol_id(&cdp_session_id)
        {
            return Err(NativeProtocolError::InvalidBinding);
        }
        Ok(Self {
            target,
            cdp_target_id,
            cdp_session_id,
        })
    }
}

/// Runtime composition for one exact Browser target and flat CDP session.
pub struct CdpNativeAdapterPort {
    target: NativeTarget,
    cdp_target_id: String,
    session_mode: CdpBrowserSessionMode,
    cdp_session_id: String,
    target_revision: String,
    snapshot_namespace: String,
    tree_depth: u32,
    next_snapshot_sequence: u64,
    next_popup_sequence: u64,
    accessibility_enabled: bool,
    popup_tracking_enabled: bool,
    managed_downloads_denied: bool,
    usable: bool,
    pending_popups: VecDeque<CdpPendingPopup>,
    last_snapshot_id: Option<NativeSnapshotId>,
    last_bounds: Option<NativeObservationBounds>,
    last_history_entry: Option<CdpHistoryEntry>,
    last_frame_tree: Option<CdpFrameTree>,
    admitted_top_level_origin: Option<String>,
    current_binding: Option<CdpSnapshotBindingV1>,
    client: CdpClient,
}

impl CdpNativeAdapterPort {
    /// Constructs one Attached-mode port without global target discovery.
    pub fn new(
        page: CdpPageBinding,
        target_revision: impl Into<String>,
        snapshot_namespace: impl Into<String>,
        tree_depth: u32,
        client: CdpClient,
    ) -> Result<Self, NativeProtocolError> {
        Self::new_with_mode(
            page,
            CdpBrowserSessionMode::Attached,
            target_revision,
            snapshot_namespace,
            tree_depth,
            client,
        )
    }

    /// Constructs one port with an explicit managed/attached discovery boundary.
    pub fn new_with_mode(
        page: CdpPageBinding,
        session_mode: CdpBrowserSessionMode,
        target_revision: impl Into<String>,
        snapshot_namespace: impl Into<String>,
        tree_depth: u32,
        client: CdpClient,
    ) -> Result<Self, NativeProtocolError> {
        let target_revision = target_revision.into();
        let snapshot_namespace = snapshot_namespace.into();
        validate_port_definition(
            &page.target,
            &target_revision,
            &snapshot_namespace,
            tree_depth,
        )?;
        Ok(Self {
            target: page.target,
            cdp_target_id: page.cdp_target_id,
            session_mode,
            cdp_session_id: page.cdp_session_id,
            target_revision,
            snapshot_namespace,
            tree_depth,
            next_snapshot_sequence: 1,
            next_popup_sequence: 1,
            accessibility_enabled: false,
            popup_tracking_enabled: false,
            managed_downloads_denied: false,
            usable: true,
            pending_popups: VecDeque::new(),
            last_snapshot_id: None,
            last_bounds: None,
            last_history_entry: None,
            last_frame_tree: None,
            admitted_top_level_origin: None,
            current_binding: None,
            client,
        })
    }

    /// Returns the oldest popup awaiting an explicit admit/reject decision.
    pub fn oldest_pending_popup(&self) -> Option<CdpPendingPopup> {
        if self.usable {
            self.pending_popups.front().cloned()
        } else {
            None
        }
    }

    /// Explicitly rejects and closes one exact pending popup target.
    pub async fn reject_pending_popup(
        &mut self,
        admission_id: &CdpPopupAdmissionId,
    ) -> Result<(), NativeProtocolError> {
        self.require_usable()?;
        let index = self.pending_popup_index(admission_id)?;
        self.close_pending_popup(index).await?;
        Ok(())
    }

    /// Attaches and admits one pending popup as a distinct opaque Runtime page.
    pub async fn admit_pending_popup(
        &mut self,
        admission_id: &CdpPopupAdmissionId,
        target: NativeTarget,
        target_revision: impl Into<String>,
        snapshot_namespace: impl Into<String>,
        tree_depth: u32,
        mut client: CdpClient,
    ) -> Result<Self, NativeProtocolError> {
        self.require_usable()?;
        let target_revision = target_revision.into();
        let snapshot_namespace = snapshot_namespace.into();
        validate_port_definition(&target, &target_revision, &snapshot_namespace, tree_depth)?;
        let NativeTarget::Browser {
            session_id: parent_session,
            page_id: parent_page,
        } = &self.target
        else {
            return Err(NativeProtocolError::TargetNotAdmitted);
        };
        let NativeTarget::Browser {
            session_id: child_session,
            page_id: child_page,
        } = &target
        else {
            return Err(NativeProtocolError::InvalidBinding);
        };
        if child_session != parent_session || child_page == parent_page {
            return Err(NativeProtocolError::InvalidBinding);
        }
        let index = self.pending_popup_index(admission_id)?;
        let pending = self.pending_popups[index].clone();
        let cdp_session_id = match client.attach_target(&pending.popup.target_id).await {
            Ok(session_id) => session_id,
            Err(error) => {
                let failure = observation_error(error);
                self.close_pending_popup(index).await?;
                return Err(failure);
            }
        };
        let admitted_origin = match popup_current_origin(&mut client, &cdp_session_id).await {
            Ok(origin) => origin,
            Err(error) => {
                self.close_pending_popup(index).await?;
                return Err(error);
            }
        };
        if admitted_origin != pending.requested_origin {
            self.close_pending_popup(index).await?;
            return Err(NativeProtocolError::BrowserOriginDenied);
        }
        let removed = self
            .pending_popups
            .remove(index)
            .ok_or(NativeProtocolError::ActionUncertain)?;
        let page = CdpPageBinding::new(target, removed.popup.target_id, cdp_session_id)?;
        let mut child = Self::new_with_mode(
            page,
            CdpBrowserSessionMode::Managed,
            target_revision,
            snapshot_namespace,
            tree_depth,
            client,
        )?;
        child.admitted_top_level_origin = Some(admitted_origin);
        Ok(child)
    }

    fn pending_popup_index(
        &self,
        admission_id: &CdpPopupAdmissionId,
    ) -> Result<usize, NativeProtocolError> {
        self.pending_popups
            .iter()
            .position(|pending| &pending.admission_id == admission_id)
            .ok_or(NativeProtocolError::TargetNotAdmitted)
    }

    fn require_usable(&self) -> Result<(), NativeProtocolError> {
        if self.usable {
            Ok(())
        } else {
            Err(NativeProtocolError::BrowserAttachmentLost)
        }
    }

    fn poison_attachment(&mut self) {
        self.usable = false;
        self.current_binding = None;
        self.pending_popups.clear();
    }

    fn observation_failure(&mut self, error: CdpTransportError) -> NativeProtocolError {
        let failure = observation_error(error);
        if failure == NativeProtocolError::BrowserAttachmentLost {
            self.poison_attachment();
        }
        failure
    }

    fn action_failure(&mut self, error: CdpTransportError) -> NativeProtocolError {
        if matches!(
            error,
            CdpTransportError::ConnectFailed
                | CdpTransportError::ConnectionLost
                | CdpTransportError::Timeout
        ) {
            self.poison_attachment();
        }
        NativeProtocolError::ActionUncertain
    }

    async fn close_pending_popup(
        &mut self,
        index: usize,
    ) -> Result<CdpPendingPopup, NativeProtocolError> {
        let pending = self
            .pending_popups
            .remove(index)
            .ok_or(NativeProtocolError::ActionUncertain)?;
        self.close_popup_or_poison(&pending.popup).await?;
        Ok(pending)
    }

    async fn close_popup_or_poison(
        &mut self,
        popup: &garive_browser_cdp::CdpPopup,
    ) -> Result<(), NativeProtocolError> {
        if self.client.close_popup(popup).await.is_err() {
            self.poison_attachment();
            return Err(NativeProtocolError::ActionUncertain);
        }
        Ok(())
    }

    fn page_target_id(&self) -> Result<&str, NativeProtocolError> {
        let NativeTarget::Browser { .. } = &self.target else {
            return Err(NativeProtocolError::TargetNotAdmitted);
        };
        Ok(&self.cdp_target_id)
    }

    async fn ensure_managed_boundaries(&mut self) -> Result<(), NativeProtocolError> {
        if self.session_mode != CdpBrowserSessionMode::Managed {
            return Ok(());
        }
        if !self.managed_downloads_denied {
            self.client
                .deny_managed_downloads()
                .await
                .map_err(|error| self.observation_failure(error))?;
            self.managed_downloads_denied = true;
        }
        if !self.popup_tracking_enabled {
            self.client
                .enable_managed_popup_tracking(&self.cdp_session_id)
                .await
                .map_err(|error| self.observation_failure(error))?;
            self.popup_tracking_enabled = true;
        }
        Ok(())
    }

    fn begin_popup_action(&mut self) -> Result<(), NativeProtocolError> {
        if self.session_mode == CdpBrowserSessionMode::Managed {
            self.client
                .begin_popup_action(&self.cdp_session_id)
                .map_err(observation_error)?;
        }
        Ok(())
    }

    async fn audit_popups(
        &mut self,
        allowed_origins: &[String],
    ) -> Result<CdpPopupAudit, NativeProtocolError> {
        if self.session_mode != CdpBrowserSessionMode::Managed {
            return Ok(CdpPopupAudit::default());
        }
        let opener_target_id = self.page_target_id()?.to_owned();
        let mut audit = CdpPopupAudit::default();
        for index in 0..=MAX_POPUPS_PER_ACTION {
            let Some(popup) = self
                .client
                .take_popup(&self.cdp_session_id, &opener_target_id)
                .await
                .map_err(|error| self.action_failure(error))?
            else {
                break;
            };
            audit.count = audit.count.saturating_add(1);
            let requested_origin = canonical_http_origin(&popup.requested_url);
            let target_origin = canonical_http_origin(&popup.target_url);
            let target_consistent = popup.target_url.is_empty()
                || popup.target_url == "about:blank"
                || target_origin == requested_origin;
            let admitted = index < MAX_POPUPS_PER_ACTION
                && target_consistent
                && requested_origin
                    .as_ref()
                    .is_some_and(|origin| allowed_origins.binary_search(origin).is_ok())
                && self.pending_popups.len() < MAX_PENDING_POPUPS;
            if !admitted {
                self.close_popup_or_poison(&popup).await?;
                audit.failure_code = Some(
                    if index >= MAX_POPUPS_PER_ACTION
                        || self.pending_popups.len() >= MAX_PENDING_POPUPS
                    {
                        NativeProtocolError::ResultBoundExceeded.code()
                    } else {
                        NativeProtocolError::BrowserOriginDenied.code()
                    }
                    .to_owned(),
                );
                continue;
            }
            if self
                .pending_popups
                .iter()
                .any(|pending| pending.popup.target_id == popup.target_id)
            {
                self.close_popup_or_poison(&popup).await?;
                return Err(NativeProtocolError::ActionUncertain);
            }
            let sequence = self.next_popup_sequence;
            self.next_popup_sequence = self
                .next_popup_sequence
                .checked_add(1)
                .ok_or(NativeProtocolError::ResultBoundExceeded)?;
            let admission_id = CdpPopupAdmissionId(format!(
                "popup-{}",
                canonical_digest(&json!({
                    "snapshot_namespace": self.snapshot_namespace,
                    "sequence": sequence,
                    "target_id": popup.target_id,
                }))?
            ));
            audit.admission_ids.push(admission_id.as_str().to_owned());
            self.pending_popups.push_back(CdpPendingPopup {
                admission_id,
                requested_origin: requested_origin.ok_or(NativeProtocolError::ReceiptInvalid)?,
                user_gesture: popup.user_gesture,
                popup,
            });
        }
        if audit.count > 0 {
            self.client
                .activate_target(&opener_target_id)
                .await
                .map_err(|error| self.action_failure(error))?;
        }
        Ok(audit)
    }

    fn resolve_action(
        &self,
        command: &NativeActionCommandV1,
    ) -> Result<ResolvedAction, NativeProtocolError> {
        if command.target != self.target {
            return Err(NativeProtocolError::TargetNotAdmitted);
        }
        let binding = self
            .current_binding
            .as_ref()
            .ok_or(NativeProtocolError::SnapshotStale)?;
        let action = command
            .prepared_input
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or(NativeProtocolError::ActionUnsupported)?;
        let node_ref = command
            .prepared_input
            .get("node_ref")
            .and_then(serde_json::Value::as_str)
            .ok_or(NativeProtocolError::ActionUnsupported)
            .and_then(NativeNodeRef::new)?;
        let target = match action {
            "click" => binding.resolve_click(
                &command.target,
                &command.expected_snapshot_id,
                &command.target_revision,
                &node_ref,
            )?,
            "type_text" => binding.resolve_type_text(
                &command.target,
                &command.expected_snapshot_id,
                &command.target_revision,
                &node_ref,
            )?,
            "clear" => binding.resolve_clear(
                &command.target,
                &command.expected_snapshot_id,
                &command.target_revision,
                &node_ref,
            )?,
            "select_option" => binding.resolve_select_option(
                &command.target,
                &command.expected_snapshot_id,
                &command.target_revision,
                &node_ref,
            )?,
            _ => return Err(NativeProtocolError::ActionUnsupported),
        };
        let text = if action == "type_text" {
            Some(
                command
                    .prepared_input
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .ok_or(NativeProtocolError::InvalidBinding)?
                    .to_owned(),
            )
        } else {
            None
        };
        let option = if action == "select_option" {
            Some(
                command
                    .prepared_input
                    .get("option")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| {
                        !value.is_empty() && value.chars().count() <= 4_096 && value.len() <= 16_384
                    })
                    .ok_or(NativeProtocolError::InvalidBinding)?
                    .to_owned(),
            )
        } else {
            None
        };
        Ok(ResolvedAction {
            action: action.into(),
            target,
            text,
            option,
            allowed_navigation_origins: allowed_navigation_origins(command)?,
        })
    }

    fn binding_for(
        &self,
        command: &NativeActionCommandV1,
        resolved: &ResolvedAction,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
        let digest = canonical_digest(&json!({
            "adapter_revision": CDP_ADAPTER_REVISION,
            "action_id": command.action_id.as_str(),
            "target": command.target,
            "snapshot_id": command.expected_snapshot_id.as_str(),
            "target_revision": command.target_revision,
            "action": resolved.action,
            "node_backend_id": resolved.target.backend_dom_node_id,
            "frame_id": resolved.target.frame_id,
            "text": resolved.text,
            "option": resolved.option,
            "allowed_navigation_origins": resolved.allowed_navigation_origins,
        }))?;
        Ok(NativeAdapterBindingV1 {
            adapter_id: ADAPTER_ID.into(),
            adapter_revision: CDP_ADAPTER_REVISION.into(),
            preflight_evidence_digest: digest,
        })
    }

    fn resolve_page_action(
        &self,
        command: &NativeActionCommandV1,
    ) -> Result<ResolvedPageAction, NativeProtocolError> {
        let binding = self
            .current_binding
            .as_ref()
            .ok_or(NativeProtocolError::SnapshotStale)?;
        binding.validate_page(
            &command.target,
            &command.expected_snapshot_id,
            &command.target_revision,
        )?;
        let action = command
            .prepared_input
            .get("action")
            .and_then(serde_json::Value::as_str)
            .ok_or(NativeProtocolError::ActionUnsupported)?;
        let kind = match action {
            "press_key" => ResolvedPageActionKind::PressKey {
                key: portable_key(
                    command
                        .prepared_input
                        .get("key")
                        .and_then(serde_json::Value::as_str)
                        .ok_or(NativeProtocolError::InvalidBinding)?,
                )?,
                focused_target: binding.resolve_focus(
                    &command.target,
                    &command.expected_snapshot_id,
                    &command.target_revision,
                )?,
            },
            "scroll" => {
                let number = |field| {
                    command
                        .prepared_input
                        .get(field)
                        .and_then(serde_json::Value::as_i64)
                        .ok_or(NativeProtocolError::InvalidBinding)
                };
                let delta_x = number("delta_x")?;
                let delta_y = number("delta_y")?;
                if (delta_x == 0 && delta_y == 0)
                    || delta_x.unsigned_abs() > 100_000
                    || delta_y.unsigned_abs() > 100_000
                {
                    return Err(NativeProtocolError::InvalidBinding);
                }
                ResolvedPageActionKind::Scroll { delta_x, delta_y }
            }
            _ => return Err(NativeProtocolError::ActionUnsupported),
        };
        Ok(ResolvedPageAction {
            kind,
            allowed_navigation_origins: allowed_navigation_origins(command)?,
        })
    }

    fn page_action_binding_for(
        &self,
        command: &NativeActionCommandV1,
        action: &ResolvedPageAction,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
        Ok(NativeAdapterBindingV1 {
            adapter_id: ADAPTER_ID.into(),
            adapter_revision: CDP_ADAPTER_REVISION.into(),
            preflight_evidence_digest: canonical_digest(&json!({
                "adapter_revision": CDP_ADAPTER_REVISION,
                "action_id": command.action_id.as_str(),
                "target": command.target,
                "snapshot_id": command.expected_snapshot_id.as_str(),
                "target_revision": command.target_revision,
                "page_action": action.evidence(),
                "allowed_navigation_origins": action.allowed_navigation_origins,
            }))?,
        })
    }

    fn resolve_history_action(
        &self,
        command: &NativeActionCommandV1,
    ) -> Result<ResolvedHistoryAction, NativeProtocolError> {
        self.current_binding
            .as_ref()
            .ok_or(NativeProtocolError::SnapshotStale)?
            .validate_page(
                &command.target,
                &command.expected_snapshot_id,
                &command.target_revision,
            )?;
        let kind = match command
            .prepared_input
            .get("action")
            .and_then(serde_json::Value::as_str)
        {
            Some("go_back") => HistoryActionKind::Back,
            Some("go_forward") => HistoryActionKind::Forward,
            Some("reload") => HistoryActionKind::Reload,
            _ => return Err(NativeProtocolError::ActionUnsupported),
        };
        Ok(ResolvedHistoryAction {
            kind,
            allowed_navigation_origins: allowed_navigation_origins(command)?,
        })
    }

    fn history_action_binding_for(
        &self,
        command: &NativeActionCommandV1,
        action: &ResolvedHistoryAction,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
        Ok(NativeAdapterBindingV1 {
            adapter_id: ADAPTER_ID.into(),
            adapter_revision: CDP_ADAPTER_REVISION.into(),
            preflight_evidence_digest: canonical_digest(&json!({
                "adapter_revision": CDP_ADAPTER_REVISION,
                "action_id": command.action_id.as_str(),
                "target": command.target,
                "snapshot_id": command.expected_snapshot_id.as_str(),
                "target_revision": command.target_revision,
                "history_action": action.kind.name(),
                "allowed_navigation_origins": action.allowed_navigation_origins,
            }))?,
        })
    }

    async fn revalidate_history_snapshot(
        &mut self,
    ) -> Result<CdpHistoryEntry, NativeProtocolError> {
        self.revalidate_frame_snapshot().await?;
        let current = self
            .client
            .current_history_entry(&self.cdp_session_id)
            .await
            .map_err(|error| self.observation_failure(error))?;
        if self.last_history_entry.as_ref() != Some(&current) {
            self.current_binding = None;
            return Err(NativeProtocolError::SnapshotStale);
        }
        Ok(current)
    }

    async fn revalidate_frame_snapshot(&mut self) -> Result<(), NativeProtocolError> {
        let current = self
            .client
            .frame_tree(&self.cdp_session_id)
            .await
            .map_err(|error| self.observation_failure(error))?;
        if self.last_frame_tree.as_ref() != Some(&current) {
            self.current_binding = None;
            return Err(NativeProtocolError::SnapshotStale);
        }
        Ok(())
    }

    async fn capture_resulting_frame_tree(&mut self) -> Result<bool, NativeProtocolError> {
        let current = self
            .client
            .frame_tree(&self.cdp_session_id)
            .await
            .map_err(|error| self.action_failure(error))?;
        let changed = self.last_frame_tree.as_ref() != Some(&current);
        if changed {
            self.target_revision = frame_revision(&self.target_revision, &current)?;
        }
        self.last_frame_tree = Some(current);
        Ok(changed)
    }

    async fn classify_password_nodes(
        &mut self,
        tree: &mut CdpAxTree,
    ) -> Result<(), NativeProtocolError> {
        for node in &mut tree.nodes {
            let password_candidate = node.role.as_deref().is_some_and(|role| {
                matches!(
                    role.to_ascii_lowercase().as_str(),
                    "textbox" | "text_field" | "textfield" | "searchbox"
                )
            });
            let Some(backend_dom_node_id) = node.backend_dom_node_id.filter(|_| password_candidate)
            else {
                continue;
            };
            if self
                .client
                .backend_node_is_password(&self.cdp_session_id, backend_dom_node_id)
                .await
                .map_err(|error| self.observation_failure(error))?
            {
                node.properties.push(CdpAxProperty {
                    name: "protected".into(),
                    value: json!({"type":"boolean","value":true}),
                });
            }
        }
        Ok(())
    }

    fn resolve_navigation(
        &self,
        command: &NativeActionCommandV1,
    ) -> Result<ResolvedNavigation, NativeProtocolError> {
        if command.target != self.target {
            return Err(NativeProtocolError::TargetNotAdmitted);
        }
        if self.current_binding.is_none()
            || self.last_snapshot_id.as_ref() != Some(&command.expected_snapshot_id)
            || command.target_revision != self.target_revision
        {
            return Err(NativeProtocolError::SnapshotStale);
        }
        let text = |field| {
            command
                .prepared_input
                .get(field)
                .and_then(serde_json::Value::as_str)
                .ok_or(NativeProtocolError::InvalidBinding)
        };
        let destination_url = text("destination_url")?.to_owned();
        let destination_origin = text("destination_origin")?.to_owned();
        if canonical_http_origin(&destination_url).as_deref() != Some(&destination_origin)
            || canonical_http_origin(&destination_origin).as_deref()
                != Some(destination_origin.as_str())
        {
            return Err(NativeProtocolError::BrowserOriginDenied);
        }
        let wait_until = match text("wait_until")? {
            "dom_content_loaded" => CdpWaitUntil::DomContentLoaded,
            "load" => CdpWaitUntil::Load,
            "network_idle" => CdpWaitUntil::NetworkIdle,
            _ => return Err(NativeProtocolError::InvalidBinding),
        };
        let number = |field| {
            command
                .prepared_input
                .get(field)
                .and_then(serde_json::Value::as_u64)
                .ok_or(NativeProtocolError::InvalidBinding)
        };
        let timeout_ms = number("timeout_ms")?;
        let max_nodes = number("max_nodes")?;
        let max_text_bytes = number("max_text_bytes")?;
        if !(1..=120_000).contains(&timeout_ms)
            || !(1..=10_000).contains(&max_nodes)
            || !(1..=1_048_576).contains(&max_text_bytes)
        {
            return Err(NativeProtocolError::InvalidBinding);
        }
        Ok(ResolvedNavigation {
            destination_url,
            destination_origin,
            wait_until,
            timeout_ms,
            max_nodes,
            max_text_bytes,
        })
    }

    fn navigation_binding_for(
        &self,
        command: &NativeActionCommandV1,
        navigation: &ResolvedNavigation,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
        Ok(NativeAdapterBindingV1 {
            adapter_id: ADAPTER_ID.into(),
            adapter_revision: CDP_ADAPTER_REVISION.into(),
            preflight_evidence_digest: canonical_digest(&json!({
                "adapter_revision": CDP_ADAPTER_REVISION,
                "action_id": command.action_id.as_str(),
                "target": command.target,
                "snapshot_id": command.expected_snapshot_id.as_str(),
                "target_revision": command.target_revision,
                "destination_url": navigation.destination_url,
                "destination_origin": navigation.destination_origin,
                "wait_until": wait_name(navigation.wait_until),
                "timeout_ms": navigation.timeout_ms,
                "max_nodes": navigation.max_nodes,
                "max_text_bytes": navigation.max_text_bytes,
            }))?,
        })
    }
}

impl NativeAdapterPort for CdpNativeAdapterPort {
    fn observe<'a>(
        &'a mut self,
        target: &'a NativeTarget,
        expected_previous_snapshot_id: Option<&'a NativeSnapshotId>,
        bounds: NativeObservationBounds,
    ) -> NativeObservationFuture<'a> {
        Box::pin(async move {
            self.require_usable()?;
            if target != &self.target {
                return Err(NativeProtocolError::TargetNotAdmitted);
            }
            if expected_previous_snapshot_id != self.last_snapshot_id.as_ref()
                && expected_previous_snapshot_id.is_some()
            {
                return Err(NativeProtocolError::SnapshotStale);
            }
            self.ensure_managed_boundaries().await?;
            if !self.accessibility_enabled {
                self.client
                    .enable_accessibility(&self.cdp_session_id)
                    .await
                    .map_err(|error| self.observation_failure(error))?;
                self.accessibility_enabled = true;
            }
            let frame_tree_before = self
                .client
                .frame_tree(&self.cdp_session_id)
                .await
                .map_err(|error| self.observation_failure(error))?;
            let mut tree = self
                .client
                .full_ax_tree(
                    &self.cdp_session_id,
                    None,
                    self.tree_depth,
                    bounds.max_nodes as usize,
                    bounds.max_text_bytes as usize,
                )
                .await
                .map_err(|error| self.observation_failure(error))?;
            let history_entry = self
                .client
                .current_history_entry(&self.cdp_session_id)
                .await
                .map_err(|error| self.observation_failure(error))?;
            self.classify_password_nodes(&mut tree).await?;
            let main_origin = canonical_http_origin(&history_entry.url)
                .ok_or(NativeProtocolError::BrowserOriginDenied)?;
            if self
                .admitted_top_level_origin
                .as_ref()
                .is_some_and(|admitted| admitted != &main_origin)
            {
                self.current_binding = None;
                return Err(NativeProtocolError::BrowserOriginDenied);
            }
            let mut frame_scope = CdpFrameScope::same_origin(&frame_tree_before, &main_origin)?;
            for frame in frame_tree_before.frames.iter().skip(1) {
                let owner = self
                    .client
                    .frame_owner_backend_node(&self.cdp_session_id, &frame.id)
                    .await
                    .map_err(|error| self.observation_failure(error))?;
                frame_scope.bind_frame_owner(&frame.id, owner)?;
                if frame_scope.admits(&frame.id) {
                    let remaining_nodes = (bounds.max_nodes as usize)
                        .checked_sub(tree.nodes.len())
                        .filter(|remaining| *remaining > 0)
                        .ok_or(NativeProtocolError::InvalidBinding)?;
                    let child = self
                        .client
                        .full_ax_tree(
                            &self.cdp_session_id,
                            Some(&frame.id),
                            self.tree_depth,
                            remaining_nodes,
                            bounds.max_text_bytes as usize,
                        )
                        .await
                        .map_err(|error| self.observation_failure(error))?;
                    let mut child = child;
                    self.classify_password_nodes(&mut child).await?;
                    tree.nodes.extend(child.nodes);
                }
            }
            let frame_tree_after = self
                .client
                .frame_tree(&self.cdp_session_id)
                .await
                .map_err(|error| self.observation_failure(error))?;
            if frame_tree_before != frame_tree_after {
                self.current_binding = None;
                return Err(NativeProtocolError::SnapshotStale);
            }
            if ax_tree_text_bytes(&tree) > bounds.max_text_bytes as usize {
                return Err(NativeProtocolError::InvalidBinding);
            }
            if self
                .last_history_entry
                .as_ref()
                .is_some_and(|previous| previous != &history_entry)
            {
                self.target_revision = history_revision(&self.target_revision, &history_entry);
            }
            let snapshot_id = NativeSnapshotId::new(format!(
                "snapshot-{:x}",
                Sha256::digest(
                    format!(
                        "{}\0{}\0{}",
                        self.snapshot_namespace, self.target_revision, self.next_snapshot_sequence
                    )
                    .as_bytes()
                )
            ))?;
            self.next_snapshot_sequence = self
                .next_snapshot_sequence
                .checked_add(1)
                .ok_or(NativeProtocolError::InvalidBinding)?;
            let mapped = map_cdp_ax_tree_with_frame_scope(
                CdpObservationContext {
                    target: self.target.clone(),
                    snapshot_id: snapshot_id.clone(),
                    target_revision: self.target_revision.clone(),
                    bounds,
                },
                &tree,
                &frame_scope,
            )?;
            self.last_snapshot_id = Some(snapshot_id);
            self.last_bounds = Some(bounds);
            self.last_history_entry = Some(history_entry);
            self.last_frame_tree = Some(frame_tree_after);
            self.admitted_top_level_origin = Some(main_origin);
            self.current_binding = Some(mapped.binding);
            Ok(mapped.observation)
        })
    }

    fn preflight_action(
        &mut self,
        command: &NativeActionCommandV1,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
        self.require_usable()?;
        if command.prepared_input.get("destination_url").is_some() {
            let navigation = self.resolve_navigation(command)?;
            return self.navigation_binding_for(command, &navigation);
        }
        if command
            .prepared_input
            .get("action")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|action| matches!(action, "press_key" | "scroll"))
        {
            let action = self.resolve_page_action(command)?;
            return self.page_action_binding_for(command, &action);
        }
        if command
            .prepared_input
            .get("action")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|action| matches!(action, "go_back" | "go_forward" | "reload"))
        {
            let action = self.resolve_history_action(command)?;
            return self.history_action_binding_for(command, &action);
        }
        let resolved = self.resolve_action(command)?;
        self.binding_for(command, &resolved)
    }

    fn dispatch_action<'a>(
        &'a mut self,
        command: &'a NativeActionCommandV1,
        binding: &'a NativeAdapterBindingV1,
    ) -> NativeActionFuture<'a> {
        Box::pin(async move {
            self.require_usable()?;
            if command.prepared_input.get("destination_url").is_some() {
                let navigation = self.resolve_navigation(command)?;
                if &self.navigation_binding_for(command, &navigation)? != binding {
                    return Err(NativeProtocolError::InvalidBinding);
                }
                self.revalidate_history_snapshot().await?;
                self.begin_popup_action()?;
                self.current_binding = None;
                let result = match tokio::time::timeout(
                    Duration::from_millis(navigation.timeout_ms),
                    self.client.navigate(
                        &self.cdp_session_id,
                        &navigation.destination_url,
                        navigation.wait_until,
                    ),
                )
                .await
                {
                    Ok(result) => result.map_err(|error| self.action_failure(error))?,
                    Err(_) => {
                        self.poison_attachment();
                        return Err(NativeProtocolError::ActionUncertain);
                    }
                };
                let prior_history_entry = self
                    .last_history_entry
                    .clone()
                    .ok_or(NativeProtocolError::ActionUncertain)?;
                let history_entry = self
                    .client
                    .current_history_entry(&self.cdp_session_id)
                    .await
                    .map_err(|error| self.action_failure(error))?;
                let (navigation_outcome, final_origin, mut failure_code) = match &result.outcome {
                    CdpNavigationOutcome::Page { final_url } => {
                        if history_entry.url != *final_url {
                            return Err(NativeProtocolError::ActionUncertain);
                        }
                        self.target_revision =
                            navigation_revision(&self.target_revision, &result, final_url);
                        let final_origin = canonical_http_origin(final_url);
                        let failure = (final_origin.as_deref()
                            != Some(navigation.destination_origin.as_str()))
                        .then(|| NativeProtocolError::BrowserOriginDenied.code().to_owned());
                        ("page", final_origin, failure)
                    }
                    CdpNavigationOutcome::Download => {
                        if !self.managed_downloads_denied || history_entry != prior_history_entry {
                            return Err(NativeProtocolError::ActionUncertain);
                        }
                        (
                            "download",
                            None,
                            Some(NativeProtocolError::ActionUnsupported.code().to_owned()),
                        )
                    }
                };
                self.last_history_entry = Some(history_entry.clone());
                let frame_changed = self.capture_resulting_frame_tree().await?;
                if navigation_outcome == "download" && frame_changed {
                    return Err(NativeProtocolError::ActionUncertain);
                }
                let popup_audit = self.audit_popups(&[]).await?;
                if popup_audit.failure_code.is_some() {
                    failure_code = popup_audit.failure_code.clone();
                }
                let terminal_classification = if failure_code.is_some() {
                    "failed"
                } else {
                    "completed"
                };
                update_admitted_origin(
                    &mut self.admitted_top_level_origin,
                    terminal_classification,
                    &final_origin,
                );
                let native_evidence_digest = canonical_digest(&json!({
                    "action_id": command.action_id.as_str(),
                    "preflight_evidence_digest": binding.preflight_evidence_digest,
                    "terminal_classification": terminal_classification,
                    "failure_code": failure_code,
                    "navigation_outcome": navigation_outcome,
                    "final_origin": final_origin,
                    "history_entry_id": history_entry.id,
                    "frame_changed": frame_changed,
                    "popup_count": popup_audit.count,
                    "pending_popup_admission_ids": popup_audit.admission_ids,
                    "target_revision": self.target_revision,
                }))
                .map_err(|_| NativeProtocolError::ActionUncertain)?;
                return Ok(NativeActionReceiptV1 {
                    action_id: command.action_id.clone(),
                    prior_snapshot_id: command.expected_snapshot_id.clone(),
                    binding: binding.clone(),
                    terminal_classification: terminal_classification.into(),
                    failure_code,
                    native_evidence_digest,
                    resulting_snapshot_id: None,
                });
            }
            if command
                .prepared_input
                .get("action")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|action| matches!(action, "go_back" | "go_forward" | "reload"))
            {
                let action = self.resolve_history_action(command)?;
                if &self.history_action_binding_for(command, &action)? != binding {
                    return Err(NativeProtocolError::InvalidBinding);
                }
                self.revalidate_frame_snapshot().await?;
                let history = self
                    .client
                    .navigation_history(&self.cdp_session_id)
                    .await
                    .map_err(|error| self.observation_failure(error))?;
                let before = history.entries[history.current_index].clone();
                if self.last_history_entry.as_ref() != Some(&before) {
                    self.current_binding = None;
                    return Err(NativeProtocolError::SnapshotStale);
                }
                let destination = match action.kind {
                    HistoryActionKind::Back => history
                        .current_index
                        .checked_sub(1)
                        .map(|index| history.entries[index].clone()),
                    HistoryActionKind::Forward => history
                        .current_index
                        .checked_add(1)
                        .filter(|index| *index < history.entries.len())
                        .map(|index| history.entries[index].clone()),
                    HistoryActionKind::Reload => Some(before.clone()),
                };
                let Some(destination) = destination else {
                    return history_failure_receipt(
                        command,
                        binding,
                        NativeProtocolError::ActionUnsupported,
                        action.kind.name(),
                        None,
                        &self.target_revision,
                    );
                };
                let destination_origin = canonical_http_origin(&destination.url);
                if !destination_origin.as_ref().is_some_and(|origin| {
                    action
                        .allowed_navigation_origins
                        .binary_search(origin)
                        .is_ok()
                }) {
                    return history_failure_receipt(
                        command,
                        binding,
                        NativeProtocolError::BrowserOriginDenied,
                        action.kind.name(),
                        destination_origin,
                        &self.target_revision,
                    );
                }
                self.begin_popup_action()?;
                self.current_binding = None;
                let final_entry = match action.kind {
                    HistoryActionKind::Back | HistoryActionKind::Forward => {
                        self.client
                            .navigate_to_history_entry(&self.cdp_session_id, destination.id)
                            .await
                    }
                    HistoryActionKind::Reload => self.client.reload(&self.cdp_session_id).await,
                }
                .map_err(|error| self.action_failure(error))?;
                self.target_revision = history_revision(&self.target_revision, &final_entry);
                self.last_history_entry = Some(final_entry.clone());
                let frame_changed = self.capture_resulting_frame_tree().await?;
                let popup_audit = self
                    .audit_popups(&action.allowed_navigation_origins)
                    .await?;
                let final_origin = canonical_http_origin(&final_entry.url);
                let allowed = final_origin.as_ref().is_some_and(|origin| {
                    action
                        .allowed_navigation_origins
                        .binary_search(origin)
                        .is_ok()
                });
                let mut failure_code =
                    (!allowed).then(|| NativeProtocolError::BrowserOriginDenied.code().to_owned());
                if popup_audit.failure_code.is_some() {
                    failure_code = popup_audit.failure_code.clone();
                }
                let terminal_classification = if allowed && failure_code.is_none() {
                    "completed"
                } else {
                    "failed"
                };
                update_admitted_origin(
                    &mut self.admitted_top_level_origin,
                    terminal_classification,
                    &final_origin,
                );
                let native_evidence_digest = canonical_digest(&json!({
                    "action_id": command.action_id.as_str(),
                    "preflight_evidence_digest": binding.preflight_evidence_digest,
                    "history_action": action.kind.name(),
                    "history_entry_id": final_entry.id,
                    "final_origin": final_origin,
                    "frame_changed": frame_changed,
                    "popup_count": popup_audit.count,
                    "pending_popup_admission_ids": popup_audit.admission_ids,
                    "terminal_classification": terminal_classification,
                    "failure_code": failure_code,
                    "target_revision": self.target_revision,
                }))?;
                return Ok(NativeActionReceiptV1 {
                    action_id: command.action_id.clone(),
                    prior_snapshot_id: command.expected_snapshot_id.clone(),
                    binding: binding.clone(),
                    terminal_classification: terminal_classification.into(),
                    failure_code,
                    native_evidence_digest,
                    resulting_snapshot_id: None,
                });
            }
            if command
                .prepared_input
                .get("action")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|action| matches!(action, "press_key" | "scroll"))
            {
                let action = self.resolve_page_action(command)?;
                if &self.page_action_binding_for(command, &action)? != binding {
                    return Err(NativeProtocolError::InvalidBinding);
                }
                if let ResolvedPageActionKind::PressKey { focused_target, .. } = &action.kind {
                    let bounds = self.last_bounds.ok_or(NativeProtocolError::SnapshotStale)?;
                    let current_focus = self
                        .client
                        .focused_backend_node(
                            &self.cdp_session_id,
                            self.tree_depth,
                            bounds.max_nodes as usize,
                            bounds.max_text_bytes as usize,
                        )
                        .await
                        .map_err(|error| self.observation_failure(error))?;
                    if current_focus != Some(focused_target.backend_dom_node_id) {
                        self.current_binding = None;
                        return Err(NativeProtocolError::FocusChanged);
                    }
                }
                let before_history = self.revalidate_history_snapshot().await?;
                let allowed_origins = action.allowed_navigation_origins.clone();
                self.begin_popup_action()?;
                self.current_binding = None;
                let result = match action.kind {
                    ResolvedPageActionKind::PressKey { key, .. } => {
                        self.client.press_key(&self.cdp_session_id, key).await
                    }
                    ResolvedPageActionKind::Scroll { delta_x, delta_y } => {
                        self.client
                            .scroll_viewport(&self.cdp_session_id, delta_x, delta_y)
                            .await
                    }
                };
                if let Err(error) = result {
                    return Err(self.action_failure(error));
                }
                let after_history = self
                    .client
                    .current_history_entry(&self.cdp_session_id)
                    .await
                    .map_err(|error| self.action_failure(error))?;
                let mut outcome = action_navigation_outcome(
                    &mut self.target_revision,
                    &before_history,
                    &after_history,
                    &allowed_origins,
                );
                self.last_history_entry = Some(after_history.clone());
                let frame_changed = self.capture_resulting_frame_tree().await?;
                let popup_audit = self.audit_popups(&allowed_origins).await?;
                apply_popup_audit(&mut outcome, &popup_audit);
                update_admitted_origin(
                    &mut self.admitted_top_level_origin,
                    outcome.terminal_classification,
                    &outcome.final_origin,
                );
                let receipt_evidence_digest = canonical_digest(&json!({
                    "action_id": command.action_id.as_str(),
                    "preflight_evidence_digest": binding.preflight_evidence_digest,
                    "terminal_classification": outcome.terminal_classification,
                    "failure_code": outcome.failure_code,
                    "history_entry_id": after_history.id,
                    "final_origin": outcome.final_origin,
                    "frame_changed": frame_changed,
                    "popup_count": popup_audit.count,
                    "pending_popup_admission_ids": popup_audit.admission_ids,
                    "target_revision": self.target_revision,
                }))?;
                return Ok(NativeActionReceiptV1 {
                    action_id: command.action_id.clone(),
                    prior_snapshot_id: command.expected_snapshot_id.clone(),
                    binding: binding.clone(),
                    terminal_classification: outcome.terminal_classification.into(),
                    failure_code: outcome.failure_code,
                    native_evidence_digest: receipt_evidence_digest,
                    resulting_snapshot_id: None,
                });
            }
            let resolved = self.resolve_action(command)?;
            if &self.binding_for(command, &resolved)? != binding {
                return Err(NativeProtocolError::InvalidBinding);
            }
            if matches!(resolved.action.as_str(), "type_text" | "clear")
                && self
                    .client
                    .backend_node_is_password(
                        &self.cdp_session_id,
                        resolved.target.backend_dom_node_id,
                    )
                    .await
                    .map_err(|error| self.observation_failure(error))?
            {
                self.current_binding = None;
                return Err(NativeProtocolError::SensitiveActionRequired);
            }
            let before_history = self.revalidate_history_snapshot().await?;
            self.begin_popup_action()?;
            self.current_binding = None;
            let mut semantic_failure = None;
            let mut selection_changed = None;
            let result = match resolved.action.as_str() {
                "click" => {
                    self.client
                        .click_backend_node(
                            &self.cdp_session_id,
                            resolved.target.backend_dom_node_id,
                        )
                        .await
                }
                "type_text" => {
                    self.client
                        .type_text_backend_node(
                            &self.cdp_session_id,
                            resolved.target.backend_dom_node_id,
                            resolved.text.as_deref().unwrap_or_default(),
                        )
                        .await
                }
                "clear" => {
                    self.client
                        .clear_backend_node(
                            &self.cdp_session_id,
                            resolved.target.backend_dom_node_id,
                        )
                        .await
                }
                "select_option" => {
                    match self
                        .client
                        .select_option_backend_node(
                            &self.cdp_session_id,
                            resolved.target.backend_dom_node_id,
                            resolved.option.as_deref().unwrap_or_default(),
                        )
                        .await
                    {
                        Ok(CdpSelectOutcome::Selected { changed }) => {
                            selection_changed = Some(changed);
                            Ok(())
                        }
                        Ok(CdpSelectOutcome::OptionUnavailable) => {
                            semantic_failure = Some(NativeProtocolError::ActionUnsupported);
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                }
                _ => return Err(NativeProtocolError::ActionUnsupported),
            };
            if let Err(error) = result {
                return Err(self.action_failure(error));
            }
            let after_history = self
                .client
                .current_history_entry(&self.cdp_session_id)
                .await
                .map_err(|error| self.action_failure(error))?;
            let mut outcome = action_navigation_outcome(
                &mut self.target_revision,
                &before_history,
                &after_history,
                &resolved.allowed_navigation_origins,
            );
            if outcome.failure_code.is_none() {
                if let Some(failure) = semantic_failure {
                    outcome.terminal_classification = "failed";
                    outcome.failure_code = Some(failure.code().to_owned());
                }
            }
            self.last_history_entry = Some(after_history.clone());
            let frame_changed = self.capture_resulting_frame_tree().await?;
            let popup_audit = self
                .audit_popups(&resolved.allowed_navigation_origins)
                .await?;
            apply_popup_audit(&mut outcome, &popup_audit);
            update_admitted_origin(
                &mut self.admitted_top_level_origin,
                outcome.terminal_classification,
                &outcome.final_origin,
            );
            let receipt_evidence_digest = canonical_digest(&json!({
                "action_id": command.action_id.as_str(),
                "preflight_evidence_digest": binding.preflight_evidence_digest,
                "terminal_classification": outcome.terminal_classification,
                "failure_code": outcome.failure_code,
                "semantic_action": resolved.action,
                "selection_changed": selection_changed,
                "history_entry_id": after_history.id,
                "final_origin": outcome.final_origin,
                "frame_changed": frame_changed,
                "popup_count": popup_audit.count,
                "pending_popup_admission_ids": popup_audit.admission_ids,
                "target_revision": self.target_revision,
            }))?;
            Ok(NativeActionReceiptV1 {
                action_id: command.action_id.clone(),
                prior_snapshot_id: command.expected_snapshot_id.clone(),
                binding: binding.clone(),
                terminal_classification: outcome.terminal_classification.into(),
                failure_code: outcome.failure_code,
                native_evidence_digest: receipt_evidence_digest,
                resulting_snapshot_id: None,
            })
        })
    }
}

struct ResolvedAction {
    action: String,
    target: CdpElementTarget,
    text: Option<String>,
    option: Option<String>,
    allowed_navigation_origins: Vec<String>,
}

struct ResolvedNavigation {
    destination_url: String,
    destination_origin: String,
    wait_until: CdpWaitUntil,
    timeout_ms: u64,
    max_nodes: u64,
    max_text_bytes: u64,
}

struct ActionNavigationOutcome {
    terminal_classification: &'static str,
    failure_code: Option<String>,
    final_origin: Option<String>,
}

#[derive(Default)]
struct CdpPopupAudit {
    count: u32,
    admission_ids: Vec<String>,
    failure_code: Option<String>,
}

fn apply_popup_audit(outcome: &mut ActionNavigationOutcome, audit: &CdpPopupAudit) {
    if let Some(failure_code) = &audit.failure_code {
        outcome.terminal_classification = "failed";
        outcome.failure_code = Some(failure_code.clone());
    }
}

fn update_admitted_origin(
    current: &mut Option<String>,
    terminal_classification: &str,
    final_origin: &Option<String>,
) {
    if terminal_classification == "completed" {
        if let Some(origin) = final_origin {
            *current = Some(origin.clone());
        }
    }
}

fn action_navigation_outcome(
    target_revision: &mut String,
    before: &CdpHistoryEntry,
    after: &CdpHistoryEntry,
    allowed_origins: &[String],
) -> ActionNavigationOutcome {
    if before == after {
        return ActionNavigationOutcome {
            terminal_classification: "completed",
            failure_code: None,
            final_origin: canonical_http_origin(&after.url),
        };
    }
    *target_revision = format!(
        "revision-{:x}",
        Sha256::digest(format!("{}\0{}\0{}", target_revision, after.id, after.url).as_bytes())
    );
    let final_origin = canonical_http_origin(&after.url);
    let allowed = final_origin
        .as_ref()
        .is_some_and(|origin| allowed_origins.binary_search(origin).is_ok());
    ActionNavigationOutcome {
        terminal_classification: if allowed { "completed" } else { "failed" },
        failure_code: (!allowed)
            .then(|| NativeProtocolError::BrowserOriginDenied.code().to_owned()),
        final_origin,
    }
}

struct ResolvedPageAction {
    kind: ResolvedPageActionKind,
    allowed_navigation_origins: Vec<String>,
}

struct ResolvedHistoryAction {
    kind: HistoryActionKind,
    allowed_navigation_origins: Vec<String>,
}

#[derive(Clone, Copy)]
enum HistoryActionKind {
    Back,
    Forward,
    Reload,
}

impl HistoryActionKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Back => "go_back",
            Self::Forward => "go_forward",
            Self::Reload => "reload",
        }
    }
}

enum ResolvedPageActionKind {
    PressKey {
        key: CdpPortableKey,
        focused_target: CdpElementTarget,
    },
    Scroll {
        delta_x: i64,
        delta_y: i64,
    },
}

impl ResolvedPageAction {
    fn evidence(&self) -> serde_json::Value {
        match &self.kind {
            ResolvedPageActionKind::PressKey {
                key,
                focused_target,
            } => json!({
                "action":"press_key",
                "key":portable_key_name(*key),
                "focused_backend_id":focused_target.backend_dom_node_id,
                "frame_id":focused_target.frame_id,
            }),
            ResolvedPageActionKind::Scroll { delta_x, delta_y } => {
                json!({"action":"scroll","delta_x":delta_x,"delta_y":delta_y})
            }
        }
    }
}

fn portable_key(value: &str) -> Result<CdpPortableKey, NativeProtocolError> {
    Ok(match value {
        "enter" => CdpPortableKey::Enter,
        "tab" => CdpPortableKey::Tab,
        "escape" => CdpPortableKey::Escape,
        "backspace" => CdpPortableKey::Backspace,
        "delete" => CdpPortableKey::Delete,
        "arrow_up" => CdpPortableKey::ArrowUp,
        "arrow_down" => CdpPortableKey::ArrowDown,
        "arrow_left" => CdpPortableKey::ArrowLeft,
        "arrow_right" => CdpPortableKey::ArrowRight,
        "home" => CdpPortableKey::Home,
        "end" => CdpPortableKey::End,
        "page_up" => CdpPortableKey::PageUp,
        "page_down" => CdpPortableKey::PageDown,
        "space" => CdpPortableKey::Space,
        _ => return Err(NativeProtocolError::InvalidBinding),
    })
}

fn allowed_navigation_origins(
    command: &NativeActionCommandV1,
) -> Result<Vec<String>, NativeProtocolError> {
    let values = command
        .prepared_input
        .get("allowed_navigation_origins")
        .and_then(serde_json::Value::as_array)
        .ok_or(NativeProtocolError::InvalidBinding)?;
    if values.len() > 16 {
        return Err(NativeProtocolError::InvalidBinding);
    }
    let mut origins = values
        .iter()
        .map(|value| {
            let origin = value.as_str().ok_or(NativeProtocolError::InvalidBinding)?;
            if canonical_http_origin(origin).as_deref() != Some(origin) {
                return Err(NativeProtocolError::BrowserOriginDenied);
            }
            Ok(origin.to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    origins.sort();
    if origins.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(NativeProtocolError::InvalidBinding);
    }
    Ok(origins)
}

fn history_failure_receipt(
    command: &NativeActionCommandV1,
    binding: &NativeAdapterBindingV1,
    error: NativeProtocolError,
    action: &str,
    destination_origin: Option<String>,
    target_revision: &str,
) -> Result<NativeActionReceiptV1, NativeProtocolError> {
    let failure_code = error.code().to_owned();
    let native_evidence_digest = canonical_digest(&json!({
        "action_id": command.action_id.as_str(),
        "preflight_evidence_digest": binding.preflight_evidence_digest,
        "history_action": action,
        "destination_origin": destination_origin,
        "terminal_classification": "failed",
        "failure_code": failure_code,
        "target_revision": target_revision,
    }))?;
    Ok(NativeActionReceiptV1 {
        action_id: command.action_id.clone(),
        prior_snapshot_id: command.expected_snapshot_id.clone(),
        binding: binding.clone(),
        terminal_classification: "failed".into(),
        failure_code: Some(failure_code),
        native_evidence_digest,
        resulting_snapshot_id: None,
    })
}

fn portable_key_name(value: CdpPortableKey) -> &'static str {
    match value {
        CdpPortableKey::Enter => "enter",
        CdpPortableKey::Tab => "tab",
        CdpPortableKey::Escape => "escape",
        CdpPortableKey::Backspace => "backspace",
        CdpPortableKey::Delete => "delete",
        CdpPortableKey::ArrowUp => "arrow_up",
        CdpPortableKey::ArrowDown => "arrow_down",
        CdpPortableKey::ArrowLeft => "arrow_left",
        CdpPortableKey::ArrowRight => "arrow_right",
        CdpPortableKey::Home => "home",
        CdpPortableKey::End => "end",
        CdpPortableKey::PageUp => "page_up",
        CdpPortableKey::PageDown => "page_down",
        CdpPortableKey::Space => "space",
    }
}

fn wait_name(value: CdpWaitUntil) -> &'static str {
    match value {
        CdpWaitUntil::DomContentLoaded => "dom_content_loaded",
        CdpWaitUntil::Load => "load",
        CdpWaitUntil::NetworkIdle => "network_idle",
    }
}

fn navigation_revision(previous: &str, result: &CdpNavigationResult, final_url: &str) -> String {
    format!(
        "revision-{:x}",
        Sha256::digest(
            format!(
                "{previous}\0{}\0{}\0{}",
                result.frame_id,
                result.loader_id.as_deref().unwrap_or_default(),
                final_url
            )
            .as_bytes()
        )
    )
}

fn history_revision(previous: &str, entry: &CdpHistoryEntry) -> String {
    format!(
        "revision-{:x}",
        Sha256::digest(format!("{previous}\0{}\0{}", entry.id, entry.url).as_bytes())
    )
}

fn frame_revision(previous: &str, tree: &CdpFrameTree) -> Result<String, NativeProtocolError> {
    let frames = tree
        .frames
        .iter()
        .map(|frame| {
            json!({
                "id": frame.id,
                "parent_id": frame.parent_id,
                "loader_id": frame.loader_id,
                "url": frame.url,
                "security_origin": frame.security_origin,
            })
        })
        .collect::<Vec<_>>();
    Ok(format!(
        "revision-{}",
        canonical_digest(&json!({
            "previous": previous,
            "main_frame_id": tree.main_frame_id,
            "frames": frames,
        }))?
    ))
}

fn ax_tree_text_bytes(tree: &CdpAxTree) -> usize {
    tree.nodes.iter().fold(0_usize, |total, node| {
        let direct = node.node_id.len()
            + node.role.as_ref().map_or(0, String::len)
            + node.name.as_ref().map_or(0, String::len)
            + node.value_summary.as_ref().map_or(0, String::len)
            + node.parent_id.as_ref().map_or(0, String::len)
            + node.frame_id.as_ref().map_or(0, String::len)
            + node.child_ids.iter().map(String::len).sum::<usize>();
        let properties = node
            .properties
            .iter()
            .map(|property| property.name.len() + property.value.to_string().len())
            .sum::<usize>();
        total.saturating_add(direct).saturating_add(properties)
    })
}

fn canonical_digest(value: &serde_json::Value) -> Result<String, NativeProtocolError> {
    let bytes = serde_jcs::to_vec(value).map_err(|_| NativeProtocolError::InvalidBinding)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn portable_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn bounded_protocol_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4_096
}

fn validate_port_definition(
    target: &NativeTarget,
    target_revision: &str,
    snapshot_namespace: &str,
    tree_depth: u32,
) -> Result<(), NativeProtocolError> {
    if !matches!(target, NativeTarget::Browser { .. })
        || !portable_token(target_revision, 128)
        || !portable_token(snapshot_namespace, 48)
        || !(1..=128).contains(&tree_depth)
    {
        Err(NativeProtocolError::InvalidBinding)
    } else {
        Ok(())
    }
}

async fn popup_current_origin(
    client: &mut CdpClient,
    cdp_session_id: &str,
) -> Result<String, NativeProtocolError> {
    let mut previous: Option<(i64, String, String)> = None;
    for attempt in 0..POPUP_ADMISSION_ATTEMPTS {
        let history = client
            .current_history_entry_if_available(cdp_session_id)
            .await
            .map_err(observation_error)?;
        let tree = client
            .frame_tree(cdp_session_id)
            .await
            .map_err(observation_error)?;
        let main = tree
            .frames
            .first()
            .ok_or(NativeProtocolError::ReceiptInvalid)?;
        if let Some(history) = history {
            if history.url == main.url {
                let origin = canonical_http_origin(&main.url)
                    .ok_or(NativeProtocolError::BrowserOriginDenied)?;
                let evidence = (history.id, history.url, main.loader_id.clone());
                if previous.as_ref() == Some(&evidence) {
                    return Ok(origin);
                }
                previous = Some(evidence);
            } else {
                previous = None;
            }
        } else {
            previous = None;
        }
        if attempt + 1 < POPUP_ADMISSION_ATTEMPTS {
            tokio::time::sleep(POPUP_ADMISSION_INTERVAL).await;
        }
    }
    Err(NativeProtocolError::BrowserAttachmentLost)
}

fn observation_error(error: CdpTransportError) -> NativeProtocolError {
    match error {
        CdpTransportError::ConnectFailed
        | CdpTransportError::ConnectionLost
        | CdpTransportError::Timeout => NativeProtocolError::BrowserAttachmentLost,
        CdpTransportError::Remote(_) | CdpTransportError::NavigationFailed => {
            NativeProtocolError::CapabilityUnavailable
        }
        _ => NativeProtocolError::ReceiptInvalid,
    }
}
