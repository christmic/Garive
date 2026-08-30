//! Concrete Runtime-owned T2 port for one explicitly attached CDP page.

use std::time::Duration;

use garive_browser_cdp::{
    CdpClient, CdpHistoryEntry, CdpNavigationResult, CdpPortableKey, CdpTransportError,
    CdpWaitUntil, CDP_ADAPTER_REVISION,
};
use garive_tools::canonical_http_origin;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    map_cdp_ax_tree_with_binding, CdpElementTarget, CdpObservationContext, CdpSnapshotBindingV1,
    NativeActionCommandV1, NativeActionFuture, NativeActionReceiptV1, NativeAdapterBindingV1,
    NativeAdapterPort, NativeNodeRef, NativeObservationBounds, NativeObservationFuture,
    NativeProtocolError, NativeSnapshotId, NativeTarget,
};

const ADAPTER_ID: &str = "garive.browser.cdp";

/// Runtime composition for one exact Browser target and flat CDP session.
pub struct CdpNativeAdapterPort {
    target: NativeTarget,
    cdp_session_id: String,
    target_revision: String,
    snapshot_namespace: String,
    tree_depth: u32,
    next_snapshot_sequence: u64,
    accessibility_enabled: bool,
    last_snapshot_id: Option<NativeSnapshotId>,
    last_bounds: Option<NativeObservationBounds>,
    current_binding: Option<CdpSnapshotBindingV1>,
    client: CdpClient,
}

impl CdpNativeAdapterPort {
    /// Constructs one port from explicit Runtime/CDP identities and a connected client.
    pub fn new(
        target: NativeTarget,
        cdp_session_id: impl Into<String>,
        target_revision: impl Into<String>,
        snapshot_namespace: impl Into<String>,
        tree_depth: u32,
        client: CdpClient,
    ) -> Result<Self, NativeProtocolError> {
        let cdp_session_id = cdp_session_id.into();
        let target_revision = target_revision.into();
        let snapshot_namespace = snapshot_namespace.into();
        if !matches!(target, NativeTarget::Browser { .. })
            || cdp_session_id.is_empty()
            || cdp_session_id.len() > 4_096
            || !portable_token(&target_revision, 128)
            || !portable_token(&snapshot_namespace, 48)
            || !(1..=128).contains(&tree_depth)
        {
            return Err(NativeProtocolError::InvalidBinding);
        }
        Ok(Self {
            target,
            cdp_session_id,
            target_revision,
            snapshot_namespace,
            tree_depth,
            next_snapshot_sequence: 1,
            accessibility_enabled: false,
            last_snapshot_id: None,
            last_bounds: None,
            current_binding: None,
            client,
        })
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
        Ok(ResolvedAction {
            action: action.into(),
            target,
            text,
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
            if target != &self.target {
                return Err(NativeProtocolError::TargetNotAdmitted);
            }
            if expected_previous_snapshot_id != self.last_snapshot_id.as_ref()
                && expected_previous_snapshot_id.is_some()
            {
                return Err(NativeProtocolError::SnapshotStale);
            }
            if !self.accessibility_enabled {
                self.client
                    .enable_accessibility(&self.cdp_session_id)
                    .await
                    .map_err(observation_error)?;
                self.accessibility_enabled = true;
            }
            let tree = self
                .client
                .full_ax_tree(
                    &self.cdp_session_id,
                    None,
                    self.tree_depth,
                    bounds.max_nodes as usize,
                    bounds.max_text_bytes as usize,
                )
                .await
                .map_err(observation_error)?;
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
            let mapped = map_cdp_ax_tree_with_binding(
                CdpObservationContext {
                    target: self.target.clone(),
                    snapshot_id: snapshot_id.clone(),
                    target_revision: self.target_revision.clone(),
                    bounds,
                },
                &tree,
            )?;
            self.last_snapshot_id = Some(snapshot_id);
            self.last_bounds = Some(bounds);
            self.current_binding = Some(mapped.binding);
            Ok(mapped.observation)
        })
    }

    fn preflight_action(
        &mut self,
        command: &NativeActionCommandV1,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError> {
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
        let resolved = self.resolve_action(command)?;
        self.binding_for(command, &resolved)
    }

    fn dispatch_action<'a>(
        &'a mut self,
        command: &'a NativeActionCommandV1,
        binding: &'a NativeAdapterBindingV1,
    ) -> NativeActionFuture<'a> {
        Box::pin(async move {
            if command.prepared_input.get("destination_url").is_some() {
                let navigation = self.resolve_navigation(command)?;
                if &self.navigation_binding_for(command, &navigation)? != binding {
                    return Err(NativeProtocolError::InvalidBinding);
                }
                self.current_binding = None;
                let result = tokio::time::timeout(
                    Duration::from_millis(navigation.timeout_ms),
                    self.client.navigate(
                        &self.cdp_session_id,
                        &navigation.destination_url,
                        navigation.wait_until,
                    ),
                )
                .await
                .map_err(|_| NativeProtocolError::ActionUncertain)?
                .map_err(|_| NativeProtocolError::ActionUncertain)?;
                self.target_revision = navigation_revision(&self.target_revision, &result);
                let final_origin = canonical_http_origin(&result.final_url);
                let failure_code =
                    if final_origin.as_deref() != Some(navigation.destination_origin.as_str()) {
                        Some(NativeProtocolError::BrowserOriginDenied.code().to_owned())
                    } else if result.is_download {
                        Some(NativeProtocolError::ActionUnsupported.code().to_owned())
                    } else {
                        None
                    };
                let terminal_classification = if failure_code.is_some() {
                    "failed"
                } else {
                    "completed"
                };
                let native_evidence_digest = canonical_digest(&json!({
                    "action_id": command.action_id.as_str(),
                    "preflight_evidence_digest": binding.preflight_evidence_digest,
                    "terminal_classification": terminal_classification,
                    "failure_code": failure_code,
                    "final_origin": final_origin,
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
                        .map_err(observation_error)?;
                    if current_focus != Some(focused_target.backend_dom_node_id) {
                        self.current_binding = None;
                        return Err(NativeProtocolError::FocusChanged);
                    }
                }
                let before_history = self
                    .client
                    .current_history_entry(&self.cdp_session_id)
                    .await
                    .map_err(observation_error)?;
                let allowed_origins = action.allowed_navigation_origins.clone();
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
                if result.is_err() {
                    return Err(NativeProtocolError::ActionUncertain);
                }
                let after_history = self
                    .client
                    .current_history_entry(&self.cdp_session_id)
                    .await
                    .map_err(|_| NativeProtocolError::ActionUncertain)?;
                let outcome = action_navigation_outcome(
                    &mut self.target_revision,
                    &before_history,
                    &after_history,
                    &allowed_origins,
                );
                let receipt_evidence_digest = canonical_digest(&json!({
                    "action_id": command.action_id.as_str(),
                    "preflight_evidence_digest": binding.preflight_evidence_digest,
                    "terminal_classification": outcome.terminal_classification,
                    "failure_code": outcome.failure_code,
                    "history_entry_id": after_history.id,
                    "final_origin": outcome.final_origin,
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
            let before_history = self
                .client
                .current_history_entry(&self.cdp_session_id)
                .await
                .map_err(observation_error)?;
            self.current_binding = None;
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
                _ => return Err(NativeProtocolError::ActionUnsupported),
            };
            if result.is_err() {
                return Err(NativeProtocolError::ActionUncertain);
            }
            let after_history = self
                .client
                .current_history_entry(&self.cdp_session_id)
                .await
                .map_err(|_| NativeProtocolError::ActionUncertain)?;
            let outcome = action_navigation_outcome(
                &mut self.target_revision,
                &before_history,
                &after_history,
                &resolved.allowed_navigation_origins,
            );
            let receipt_evidence_digest = canonical_digest(&json!({
                "action_id": command.action_id.as_str(),
                "preflight_evidence_digest": binding.preflight_evidence_digest,
                "terminal_classification": outcome.terminal_classification,
                "failure_code": outcome.failure_code,
                "history_entry_id": after_history.id,
                "final_origin": outcome.final_origin,
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

fn navigation_revision(previous: &str, result: &CdpNavigationResult) -> String {
    format!(
        "revision-{:x}",
        Sha256::digest(
            format!(
                "{previous}\0{}\0{}\0{}",
                result.frame_id,
                result.loader_id.as_deref().unwrap_or_default(),
                result.final_url
            )
            .as_bytes()
        )
    )
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
