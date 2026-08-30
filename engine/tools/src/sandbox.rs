//! Portable F0 sandbox enforcement requirements.

use std::collections::BTreeSet;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{ExecutionCapability, PreparationError, PreparationErrorCode};

/// Closed canonical set of enforcement controls an executor must prove.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxControl {
    /// Restrict filesystem operations to the exact granted workspace scope.
    FilesystemScope,
    /// Prevent symbolic-link or equivalent traversal outside that scope.
    SymlinkContainment,
    /// Contain spawned processes and their descendants.
    ProcessContainment,
    /// Pass an argv vector without implicit shell parsing.
    StructuredArguments,
    /// Construct child environments from an explicit allowlist.
    EnvironmentAllowlist,
    /// Restrict network operations to exact granted origins.
    NetworkOriginScope,
    /// Re-authorize every redirect destination.
    RedirectRevalidation,
    /// Restrict browser operations to the exact admitted session and page.
    BrowserSessionScope,
    /// Restrict desktop operations to the exact admitted application and window.
    NativeTargetScope,
    /// Bind each action to the exact prior semantic observation.
    SnapshotBinding,
    /// Revalidate native focus and overlay posture immediately before input.
    FocusRevalidation,
    /// Restrict capture to admitted targets with redaction and retention bounds.
    ScreenCaptureScope,
    /// Enforce the declared resource ceilings.
    ResourceLimits,
}

impl SandboxControl {
    /// Returns the stable portable control name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FilesystemScope => "filesystem_scope",
            Self::SymlinkContainment => "symlink_containment",
            Self::ProcessContainment => "process_containment",
            Self::StructuredArguments => "structured_arguments",
            Self::EnvironmentAllowlist => "environment_allowlist",
            Self::NetworkOriginScope => "network_origin_scope",
            Self::RedirectRevalidation => "redirect_revalidation",
            Self::BrowserSessionScope => "browser_session_scope",
            Self::NativeTargetScope => "native_target_scope",
            Self::SnapshotBinding => "snapshot_binding",
            Self::FocusRevalidation => "focus_revalidation",
            Self::ScreenCaptureScope => "screen_capture_scope",
            Self::ResourceLimits => "resource_limits",
        }
    }
}

/// Validated immutable F0 requirement profile for one Tool revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SandboxRequirementsV1 {
    contract: &'static str,
    version: u8,
    controls: BTreeSet<SandboxControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_processes: Option<u32>,
    max_open_files: u32,
}

impl SandboxRequirementsV1 {
    /// Validates capability-specific controls, uniqueness and non-zero bounds.
    pub fn new(
        capabilities: impl IntoIterator<Item = ExecutionCapability>,
        controls: impl IntoIterator<Item = SandboxControl>,
        max_processes: Option<u32>,
        max_open_files: u32,
    ) -> Result<Self, PreparationError> {
        let capabilities: BTreeSet<_> = capabilities.into_iter().collect();
        let control_values: Vec<_> = controls.into_iter().collect();
        let controls: BTreeSet<_> = control_values.iter().copied().collect();
        let filesystem = capabilities.contains(&ExecutionCapability::FilesystemRead)
            || capabilities.contains(&ExecutionCapability::FilesystemWrite);
        let process = capabilities.contains(&ExecutionCapability::Process);
        let network = capabilities.contains(&ExecutionCapability::Network);
        let browser = capabilities.contains(&ExecutionCapability::BrowserObserve)
            || capabilities.contains(&ExecutionCapability::BrowserAct);
        let computer = capabilities.contains(&ExecutionCapability::ComputerObserve)
            || capabilities.contains(&ExecutionCapability::ComputerAct);
        let computer_act = capabilities.contains(&ExecutionCapability::ComputerAct);
        let valid = max_open_files != 0
            && !controls.is_empty()
            && control_values.len() == controls.len()
            && process == max_processes.is_some()
            && max_processes.is_none_or(|value| value != 0)
            && (!filesystem
                || required(
                    &controls,
                    &[
                        SandboxControl::FilesystemScope,
                        SandboxControl::SymlinkContainment,
                        SandboxControl::ResourceLimits,
                    ],
                ))
            && (!process
                || required(
                    &controls,
                    &[
                        SandboxControl::ProcessContainment,
                        SandboxControl::StructuredArguments,
                        SandboxControl::EnvironmentAllowlist,
                        SandboxControl::ResourceLimits,
                    ],
                ))
            && (!network
                || required(
                    &controls,
                    &[
                        SandboxControl::NetworkOriginScope,
                        SandboxControl::RedirectRevalidation,
                        SandboxControl::ResourceLimits,
                    ],
                ))
            && (!browser
                || required(
                    &controls,
                    &[
                        SandboxControl::BrowserSessionScope,
                        SandboxControl::SnapshotBinding,
                        SandboxControl::ResourceLimits,
                    ],
                ))
            && (!computer
                || required(
                    &controls,
                    &[
                        SandboxControl::NativeTargetScope,
                        SandboxControl::SnapshotBinding,
                        SandboxControl::ResourceLimits,
                    ],
                ))
            && (!computer_act || controls.contains(&SandboxControl::FocusRevalidation));
        if !valid {
            return Err(PreparationError::new(
                PreparationErrorCode::SandboxRequirementInvalid,
            ));
        }
        Ok(Self {
            contract: "garive.sandbox-requirements",
            version: 1,
            controls,
            max_processes,
            max_open_files,
        })
    }

    /// Returns controls in their canonical enum order.
    pub fn controls(&self) -> impl ExactSizeIterator<Item = SandboxControl> + '_ {
        self.controls.iter().copied()
    }

    /// Returns the process ceiling, present only for process-capable tools.
    pub const fn max_processes(&self) -> Option<u32> {
        self.max_processes
    }

    /// Returns the non-zero open-file ceiling.
    pub const fn max_open_files(&self) -> u32 {
        self.max_open_files
    }

    /// Returns whether this executor profile proves every requested control and tighter limits.
    pub fn is_covered_by(&self, executor: &Self) -> bool {
        self.controls.is_subset(&executor.controls)
            && executor.max_open_files <= self.max_open_files
            && match (self.max_processes, executor.max_processes) {
                (Some(requested), Some(enforced)) => enforced <= requested,
                (None, None) => true,
                _ => false,
            }
    }

    /// Revalidates this frozen profile against one exact Tool capability set.
    pub fn validate_for(
        &self,
        capabilities: impl IntoIterator<Item = ExecutionCapability>,
    ) -> Result<(), PreparationError> {
        Self::new(
            capabilities,
            self.controls.iter().copied(),
            self.max_processes,
            self.max_open_files,
        )
        .map(|_| ())
    }

    /// Returns lowercase SHA-256 over the RFC 8785 canonical profile.
    pub fn digest(&self) -> Result<String, PreparationError> {
        let bytes = serde_jcs::to_vec(self)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

fn required(controls: &BTreeSet<SandboxControl>, required: &[SandboxControl]) -> bool {
    required.iter().all(|control| controls.contains(control))
}
