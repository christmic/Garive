//! Concrete Runtime-owned T2 port for one explicitly attached CDP page.

use std::time::Duration;

use garive_browser_cdp::{
    CdpClient, CdpNavigationResult, CdpTransportError, CdpWaitUntil, CDP_ADAPTER_REVISION,
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
        }))?;
        Ok(NativeAdapterBindingV1 {
            adapter_id: ADAPTER_ID.into(),
            adapter_revision: CDP_ADAPTER_REVISION.into(),
            preflight_evidence_digest: digest,
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
            let resolved = self.resolve_action(command)?;
            if &self.binding_for(command, &resolved)? != binding {
                return Err(NativeProtocolError::InvalidBinding);
            }
            let receipt_evidence_digest = canonical_digest(&json!({
                "action_id": command.action_id.as_str(),
                "preflight_evidence_digest": binding.preflight_evidence_digest,
                "terminal_classification": "completed",
            }))?;
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
            Ok(NativeActionReceiptV1 {
                action_id: command.action_id.clone(),
                prior_snapshot_id: command.expected_snapshot_id.clone(),
                binding: binding.clone(),
                terminal_classification: "completed".into(),
                failure_code: None,
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
}

struct ResolvedNavigation {
    destination_url: String,
    destination_origin: String,
    wait_until: CdpWaitUntil,
    timeout_ms: u64,
    max_nodes: u64,
    max_text_bytes: u64,
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
