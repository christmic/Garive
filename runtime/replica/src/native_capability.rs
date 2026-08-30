//! Platform-neutral Runtime boundary for Browser and Computer Use adapters.

use std::{future::Future, pin::Pin};

use serde::Serialize;

macro_rules! opaque_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Constructs a bounded portable opaque identity.
            pub fn new(value: impl Into<String>) -> Result<Self, NativeProtocolError> {
                let value = value.into();
                if !portable_token(&value) {
                    return Err(NativeProtocolError::InvalidBinding);
                }
                Ok(Self(value))
            }

            /// Returns the opaque identity without changing its type.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

opaque_id!(
    BrowserSessionId,
    "Runtime identity for one Browser session."
);
opaque_id!(BrowserPageId, "Runtime identity for one Browser page.");
opaque_id!(
    DesktopSessionId,
    "Runtime identity for one desktop session."
);
opaque_id!(
    ApplicationId,
    "Runtime identity for one admitted application."
);
opaque_id!(WindowId, "Runtime identity for one admitted native window.");
opaque_id!(
    NativeSnapshotId,
    "Runtime identity for one immutable observation."
);
opaque_id!(NativeNodeRef, "Snapshot-local semantic node capability.");
opaque_id!(
    NativeActionId,
    "Runtime identity for one native action attempt."
);

/// Exact target admitted to a native adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum NativeTarget {
    /// One page inside an explicit Browser session.
    Browser {
        /// Browser session identity.
        session_id: BrowserSessionId,
        /// Page identity.
        page_id: BrowserPageId,
    },
    /// One native window inside an explicit Desktop session.
    Computer {
        /// Desktop session identity.
        session_id: DesktopSessionId,
        /// Application identity.
        application_id: ApplicationId,
        /// Window identity.
        window_id: WindowId,
    },
}

/// Sensitivity attached to a semantic field after Runtime redaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeSensitivity {
    /// Ordinary content admitted to Core.
    Public,
    /// Private content admitted under the exact session policy.
    Private,
    /// Protected content whose value has been replaced by a redaction token.
    Redacted,
}

/// One provider-neutral semantic node in parent-before-child order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeSemanticNode {
    /// Snapshot-local reference.
    pub node_ref: NativeNodeRef,
    /// Optional parent reference; roots have no parent.
    pub parent_ref: Option<NativeNodeRef>,
    /// Portable normalized role token.
    pub role: String,
    /// Optional bounded accessible name.
    pub name: Option<String>,
    /// Optional bounded value summary.
    pub value_summary: Option<String>,
    /// Sorted, unique portable state tokens.
    pub states: Vec<String>,
    /// Sorted, unique portable action tokens.
    pub actions: Vec<String>,
    /// Sensitivity of text-bearing fields.
    pub sensitivity: NativeSensitivity,
}

/// Bounds actually applied while collecting an observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct NativeObservationBounds {
    /// Maximum semantic node count.
    pub max_nodes: u32,
    /// Maximum combined UTF-8 text bytes.
    pub max_text_bytes: u32,
}

/// Immutable bounded observation stored before Core can use node references.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeObservationV1 {
    /// Exact observed target.
    pub target: NativeTarget,
    /// New Runtime snapshot identity.
    pub snapshot_id: NativeSnapshotId,
    /// Adapter-observed target revision.
    pub target_revision: String,
    /// Flat parent-before-child semantic tree.
    pub nodes: Vec<NativeSemanticNode>,
    /// Optional focused node in this snapshot.
    pub focused_node: Option<NativeNodeRef>,
    /// Optional separately governed screenshot/content reference.
    pub screenshot_reference: Option<String>,
    /// Count of fields removed or replaced before persistence.
    pub redacted_field_count: u32,
    /// Exact collection bounds.
    pub bounds: NativeObservationBounds,
}

impl NativeObservationV1 {
    /// Validates identity scope, tree order, uniqueness and all declared bounds.
    pub fn validate(&self) -> Result<(), NativeProtocolError> {
        if !portable_token(&self.target_revision)
            || self.bounds.max_nodes == 0
            || self.bounds.max_nodes > 10_000
            || self.bounds.max_text_bytes == 0
            || self.bounds.max_text_bytes > 1_048_576
            || self.nodes.len() > self.bounds.max_nodes as usize
            || self
                .screenshot_reference
                .as_deref()
                .is_some_and(|value| value.is_empty())
        {
            return Err(NativeProtocolError::InvalidBinding);
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut text_bytes = 0_usize;
        for node in &self.nodes {
            if !portable_token(&node.role)
                || !ordered_tokens(&node.states)
                || !ordered_tokens(&node.actions)
                || !seen.insert(node.node_ref.as_str())
                || node
                    .parent_ref
                    .as_ref()
                    .is_some_and(|parent| !seen.contains(parent.as_str()))
            {
                return Err(NativeProtocolError::InvalidBinding);
            }
            text_bytes = text_bytes
                .checked_add(node.role.len())
                .and_then(|value| value.checked_add(optional_len(&node.name)))
                .and_then(|value| value.checked_add(optional_len(&node.value_summary)))
                .ok_or(NativeProtocolError::ResultBoundExceeded)?;
        }
        if text_bytes > self.bounds.max_text_bytes as usize {
            return Err(NativeProtocolError::ResultBoundExceeded);
        }
        if self
            .focused_node
            .as_ref()
            .is_some_and(|node| !seen.contains(node.as_str()))
        {
            return Err(NativeProtocolError::InvalidBinding);
        }
        Ok(())
    }
}

/// Snapshot-bound, already-authorized native adapter command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeActionCommandV1 {
    /// Runtime action identity.
    pub action_id: NativeActionId,
    /// Exact admitted target.
    pub target: NativeTarget,
    /// Observation whose ephemeral references the action may use.
    pub expected_snapshot_id: NativeSnapshotId,
    /// Exact target revision seen in that observation.
    pub target_revision: String,
    /// Canonical Engine-prepared input; the adapter may not reinterpret policy.
    pub prepared_input: serde_json::Value,
}

/// Adapter identity proven during non-dispatching preflight.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeAdapterBindingV1 {
    /// Stable adapter identity.
    pub adapter_id: String,
    /// Exact adapter revision.
    pub adapter_revision: String,
    /// Permission/focus/snapshot evidence digest.
    pub preflight_evidence_digest: String,
}

impl NativeAdapterBindingV1 {
    /// Validates stable adapter identity and canonical evidence digest.
    pub fn validate(&self) -> Result<(), NativeProtocolError> {
        if self.adapter_id.is_empty()
            || self.adapter_revision.is_empty()
            || !sha256_digest(&self.preflight_evidence_digest)
        {
            Err(NativeProtocolError::InvalidBinding)
        } else {
            Ok(())
        }
    }
}

/// Trustworthy native terminal result returned after one dispatch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NativeActionReceiptV1 {
    /// Exact action identity.
    pub action_id: NativeActionId,
    /// Exact prior snapshot.
    pub prior_snapshot_id: NativeSnapshotId,
    /// Adapter identity used for dispatch.
    pub binding: NativeAdapterBindingV1,
    /// Stable terminal classification.
    pub terminal_classification: String,
    /// Digest of bounded native evidence.
    pub native_evidence_digest: String,
    /// Optional resulting observation identity.
    pub resulting_snapshot_id: Option<NativeSnapshotId>,
}

impl NativeActionReceiptV1 {
    /// Validates adapter evidence and the closed terminal classification.
    pub fn validate(&self) -> Result<(), NativeProtocolError> {
        self.binding.validate()?;
        if !matches!(
            self.terminal_classification.as_str(),
            "completed" | "failed"
        ) || !sha256_digest(&self.native_evidence_digest)
        {
            Err(NativeProtocolError::ReceiptInvalid)
        } else {
            Ok(())
        }
    }
}

/// Stable Runtime/adapter compatibility failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeProtocolError {
    /// Invalid identity, shape or cross-object binding.
    InvalidBinding,
    /// Native capability is not present on this host.
    CapabilityUnavailable,
    /// Required operating-system permission has not been granted.
    PermissionRequired,
    /// Previously granted permission was revoked.
    PermissionRevoked,
    /// Target is outside the admitted native scope.
    TargetNotAdmitted,
    /// Expected observation is no longer current.
    SnapshotStale,
    /// Snapshot-local node reference is stale.
    NodeStale,
    /// Requested semantic/native action is unsupported.
    ActionUnsupported,
    /// Focus ownership changed before dispatch.
    FocusChanged,
    /// Browser origin is outside the exact admitted set.
    BrowserOriginDenied,
    /// Cross-origin frame is intentionally opaque.
    BrowserFrameOpaque,
    /// Verified Browser attachment was lost.
    BrowserAttachmentLost,
    /// Current target classification requires a new interaction.
    SensitiveActionRequired,
    /// Adapter result exceeded declared bounds.
    ResultBoundExceeded,
    /// Adapter receipt failed exact binding validation.
    ReceiptInvalid,
    /// Dispatch crossed the native boundary without trustworthy terminal evidence.
    ActionUncertain,
}

impl NativeProtocolError {
    /// Returns the frozen compatibility code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidBinding => "native_binding_invalid",
            Self::CapabilityUnavailable => "native_capability_unavailable",
            Self::PermissionRequired => "native_permission_required",
            Self::PermissionRevoked => "native_permission_revoked",
            Self::TargetNotAdmitted => "native_target_not_admitted",
            Self::SnapshotStale => "native_snapshot_stale",
            Self::NodeStale => "native_node_stale",
            Self::ActionUnsupported => "native_action_unsupported",
            Self::FocusChanged => "native_focus_changed",
            Self::BrowserOriginDenied => "browser_origin_denied",
            Self::BrowserFrameOpaque => "browser_frame_opaque",
            Self::BrowserAttachmentLost => "browser_attachment_lost",
            Self::SensitiveActionRequired => "native_sensitive_action_required",
            Self::ResultBoundExceeded => "native_result_bound_exceeded",
            Self::ReceiptInvalid => "native_receipt_invalid",
            Self::ActionUncertain => "native_action_uncertain",
        }
    }
}

/// Asynchronous native observation result.
pub type NativeObservationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<NativeObservationV1, NativeProtocolError>> + Send + 'a>>;
/// Asynchronous native action result after dispatch.
pub type NativeActionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<NativeActionReceiptV1, NativeProtocolError>> + Send + 'a>>;

/// Narrow platform adapter boundary; it owns mechanics but never authority.
pub trait NativeAdapterPort: Send {
    /// Collects one bounded observation inside an already-admitted target scope.
    fn observe<'a>(
        &'a mut self,
        target: &'a NativeTarget,
        expected_previous_snapshot_id: Option<&'a NativeSnapshotId>,
        bounds: NativeObservationBounds,
    ) -> NativeObservationFuture<'a>;

    /// Revalidates permission, target, snapshot, focus and sensitivity without dispatch.
    fn preflight_action(
        &mut self,
        command: &NativeActionCommandV1,
    ) -> Result<NativeAdapterBindingV1, NativeProtocolError>;

    /// Crosses the native boundary exactly once after governed Started commits.
    fn dispatch_action<'a>(
        &'a mut self,
        command: &'a NativeActionCommandV1,
        binding: &'a NativeAdapterBindingV1,
    ) -> NativeActionFuture<'a>;
}

fn portable_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn ordered_tokens(values: &[String]) -> bool {
    values.iter().all(|value| portable_token(value))
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn optional_len(value: &Option<String>) -> usize {
    value.as_ref().map_or(0, String::len)
}

fn sha256_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
