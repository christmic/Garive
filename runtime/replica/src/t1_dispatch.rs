//! Shared deterministic dispatch binding for every built-in T1 executor.

use garive_tools::ToolInvocationId;
use sha2::{Digest, Sha256};

use crate::{T1_PATCH_EXECUTOR_ID, T1_PROCESS_EXECUTOR_ID, T1_WORKSPACE_EXECUTOR_ID};

/// Derives the exact dispatch-attempt identity required by one T1 executor.
pub fn t1_dispatch_attempt_id(
    executor_id: &str,
    invocation_id: &ToolInvocationId,
) -> Option<String> {
    let digest = format!("{:x}", Sha256::digest(invocation_id.as_str().as_bytes()));
    match executor_id {
        T1_WORKSPACE_EXECUTOR_ID => Some(format!("dispatch-{digest}")),
        T1_PATCH_EXECUTOR_ID => Some(format!("patch-dispatch-{}", &digest[..24])),
        T1_PROCESS_EXECUTOR_ID => Some(format!("process-dispatch-{}", &digest[..24])),
        _ => None,
    }
}
