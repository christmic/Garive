use crate::{
    AgentInput, AgentOutput, BenchError, BenchErrorCode, BenchFuture, IntakeAdapter, PatchAdapter,
    SweCase, WorkspaceLease,
};

/// Gold-free exact SWE case intake for Garive's Agent driver.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactSweIntake;

impl IntakeAdapter for ExactSweIntake {
    fn translate<'a>(
        &'a self,
        case: &'a SweCase,
        workspace: &'a WorkspaceLease,
    ) -> BenchFuture<'a, AgentInput> {
        Box::pin(async move {
            if workspace.case_id != case.instance_id.as_str()
                || workspace.base_commit != case.base_commit
                || workspace.handle.is_empty()
            {
                return Err(BenchError::from_port(BenchErrorCode::InfrastructureFailure));
            }
            Ok(AgentInput {
                payload: case.problem_statement.clone(),
                repository: case.repository.clone(),
                base_commit: case.base_commit.clone(),
                workspace_handle: workspace.handle.clone(),
            })
        })
    }
}

/// Strict V1 adapter for Agent output that already contains a unified diff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnifiedDiffPatchAdapter {
    max_patch_bytes: usize,
}

impl UnifiedDiffPatchAdapter {
    /// Creates an adapter with a non-zero complete-patch bound.
    pub fn new(max_patch_bytes: usize) -> Result<Self, BenchError> {
        if max_patch_bytes == 0 {
            Err(BenchError::new(BenchErrorCode::InvalidLimits))
        } else {
            Ok(Self { max_patch_bytes })
        }
    }
}

impl PatchAdapter for UnifiedDiffPatchAdapter {
    fn translate<'a>(&'a self, output: &'a AgentOutput, _: &'a SweCase) -> BenchFuture<'a, String> {
        Box::pin(async move {
            validate_patch(&output.raw, self.max_patch_bytes)?;
            Ok(output.raw.clone())
        })
    }
}

fn validate_patch(value: &str, maximum: usize) -> Result<(), BenchError> {
    if value.is_empty()
        || value.len() > maximum
        || value.contains('\0')
        || value.contains('\r')
        || !value.ends_with('\n')
        || value.contains("GIT binary patch")
        || value.contains("Binary files ")
    {
        return Err(invalid_patch());
    }
    let mut headers = 0;
    for line in value.lines() {
        if let Some(paths) = line.strip_prefix("diff --git ") {
            let mut parts = paths.split(' ');
            let left = parts.next().ok_or_else(invalid_patch)?;
            let right = parts.next().ok_or_else(invalid_patch)?;
            if parts.next().is_some()
                || !left.starts_with("a/")
                || !right.starts_with("b/")
                || !valid_path(&left[2..])
                || !valid_path(&right[2..])
            {
                return Err(invalid_patch());
            }
            headers += 1;
        }
    }
    if headers == 0 || !value.starts_with("diff --git ") {
        return Err(invalid_patch());
    }
    Ok(())
}

fn valid_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".." | ".git"))
}

fn invalid_patch() -> BenchError {
    BenchError::from_port(BenchErrorCode::InvalidPatch)
}
