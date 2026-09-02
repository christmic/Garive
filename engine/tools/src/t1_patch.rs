//! Deterministic parser and in-memory applier for the T1 patch subset.

use std::collections::BTreeSet;

/// Stable failure returned while parsing or applying one T1 patch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum T1PatchError {
    /// The patch is outside the admitted revision-1 grammar.
    InvalidSyntax,
    /// The requested target is absent from the patch.
    TargetMissing,
    /// A hunk anchor is absent, repeated, or out of order in current content.
    ContextMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineKind {
    Context,
    Add,
    Remove,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PatchLine {
    kind: LineKind,
    text: String,
    no_newline: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Hunk(Vec<PatchLine>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Target {
    path: String,
    hunks: Vec<Hunk>,
}

/// Returns the canonical unique target set after validating the full grammar.
pub fn t1_patch_targets(patch: &str) -> Result<BTreeSet<String>, T1PatchError> {
    Ok(parse(patch)?
        .into_iter()
        .map(|target| target.path)
        .collect())
}

/// Applies the exact target hunks to bounded UTF-8 content without I/O.
pub fn apply_t1_patch(patch: &str, path: &str, current: &str) -> Result<String, T1PatchError> {
    let target = parse(patch)?
        .into_iter()
        .find(|target| target.path == path)
        .ok_or(T1PatchError::TargetMissing)?;
    let terminal_newline = current.ends_with('\n');
    let mut lines = current
        .strip_suffix('\n')
        .unwrap_or(current)
        .split('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if current.is_empty() {
        lines.clear();
    }
    let mut cursor = 0usize;
    let mut result_terminal_newline = terminal_newline;
    for (hunk_index, hunk) in target.hunks.iter().enumerate() {
        let before = hunk
            .0
            .iter()
            .filter(|line| line.kind != LineKind::Add)
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>();
        if before.is_empty() {
            return Err(T1PatchError::InvalidSyntax);
        }
        let positions = (cursor..=lines.len().saturating_sub(before.len()))
            .filter(|start| {
                lines
                    .get(*start..start + before.len())
                    .is_some_and(|slice| {
                        slice.iter().map(String::as_str).eq(before.iter().copied())
                    })
            })
            .take(2)
            .collect::<Vec<_>>();
        let [start] = positions.as_slice() else {
            return Err(T1PatchError::ContextMismatch);
        };
        if hunk.0.iter().any(|line| line.no_newline) {
            let final_hunk = hunk_index + 1 == target.hunks.len();
            let final_line = hunk.0.last().is_some_and(|line| line.no_newline);
            if !final_hunk || !final_line || start + before.len() != lines.len() {
                return Err(T1PatchError::InvalidSyntax);
            }
            if hunk
                .0
                .iter()
                .any(|line| line.no_newline && line.kind != LineKind::Add)
                && terminal_newline
            {
                return Err(T1PatchError::ContextMismatch);
            }
            result_terminal_newline = false;
        }
        let after = hunk
            .0
            .iter()
            .filter(|line| line.kind != LineKind::Remove)
            .map(|line| line.text.clone())
            .collect::<Vec<_>>();
        let end = start + before.len();
        lines.splice(*start..end, after);
        cursor = start
            + hunk
                .0
                .iter()
                .filter(|line| line.kind != LineKind::Remove)
                .count();
    }
    let mut result = lines.join("\n");
    if result_terminal_newline {
        result.push('\n');
    }
    Ok(result)
}

fn parse(patch: &str) -> Result<Vec<Target>, T1PatchError> {
    if patch.starts_with("*** Begin Patch\n") {
        parse_garive_patch(patch)
    } else {
        parse_unified_diff(patch)
    }
}

fn parse_garive_patch(patch: &str) -> Result<Vec<Target>, T1PatchError> {
    let body = patch
        .strip_prefix("*** Begin Patch\n")
        .and_then(|value| {
            value
                .strip_suffix("\n*** End Patch")
                .or_else(|| value.strip_suffix("\n*** End Patch\n"))
        })
        .ok_or(T1PatchError::InvalidSyntax)?;
    let mut targets = Vec::<Target>::new();
    let mut current_hunk: Option<Hunk> = None;
    for raw in body.lines() {
        if let Some(path) = raw.strip_prefix("*** Update File: ") {
            finish_hunk(&mut targets, &mut current_hunk)?;
            if path.is_empty() || path == "." || targets.iter().any(|target| target.path == path) {
                return Err(T1PatchError::InvalidSyntax);
            }
            targets.push(Target {
                path: path.to_owned(),
                hunks: Vec::new(),
            });
        } else if raw == "@@" {
            finish_hunk(&mut targets, &mut current_hunk)?;
            if targets.is_empty() {
                return Err(T1PatchError::InvalidSyntax);
            }
            current_hunk = Some(Hunk(Vec::new()));
        } else if raw == "\\ No newline at end of file" {
            let line = current_hunk
                .as_mut()
                .and_then(|hunk| hunk.0.last_mut())
                .ok_or(T1PatchError::InvalidSyntax)?;
            if line.no_newline {
                return Err(T1PatchError::InvalidSyntax);
            }
            line.no_newline = true;
        } else {
            let (kind, text) = match raw.as_bytes().first() {
                Some(b' ') => (LineKind::Context, &raw[1..]),
                Some(b'+') => (LineKind::Add, &raw[1..]),
                Some(b'-') => (LineKind::Remove, &raw[1..]),
                _ => return Err(T1PatchError::InvalidSyntax),
            };
            current_hunk
                .as_mut()
                .ok_or(T1PatchError::InvalidSyntax)?
                .0
                .push(PatchLine {
                    kind,
                    text: text.to_owned(),
                    no_newline: false,
                });
        }
    }
    finish_hunk(&mut targets, &mut current_hunk)?;
    if targets.is_empty() || targets.iter().any(|target| target.hunks.is_empty()) {
        return Err(T1PatchError::InvalidSyntax);
    }
    Ok(targets)
}

fn parse_unified_diff(patch: &str) -> Result<Vec<Target>, T1PatchError> {
    let mut lines = patch.lines().peekable();
    let mut targets = Vec::<Target>::new();
    let mut current_hunk: Option<Hunk> = None;
    while let Some(raw) = lines.next() {
        if raw.starts_with("diff --git ") {
            finish_hunk(&mut targets, &mut current_hunk)?;
            continue;
        }
        let Some(old_path) = raw.strip_prefix("--- ") else {
            return Err(T1PatchError::InvalidSyntax);
        };
        finish_hunk(&mut targets, &mut current_hunk)?;
        let new_header = lines.next().ok_or(T1PatchError::InvalidSyntax)?;
        let new_path = new_header
            .strip_prefix("+++ ")
            .ok_or(T1PatchError::InvalidSyntax)?;
        let old_path = unified_path(old_path)?;
        let new_path = unified_path(new_path)?;
        if old_path != new_path || targets.iter().any(|target| target.path == new_path) {
            return Err(T1PatchError::InvalidSyntax);
        }
        targets.push(Target {
            path: new_path.to_owned(),
            hunks: Vec::new(),
        });
        while let Some(raw) = lines.peek().copied() {
            if raw.starts_with("diff --git ") || raw.starts_with("--- ") {
                break;
            }
            let raw = lines.next().ok_or(T1PatchError::InvalidSyntax)?;
            if raw.starts_with("@@ ") && raw[3..].contains(" @@") {
                finish_hunk(&mut targets, &mut current_hunk)?;
                current_hunk = Some(Hunk(Vec::new()));
            } else if raw == "\\ No newline at end of file" {
                let line = current_hunk
                    .as_mut()
                    .and_then(|hunk| hunk.0.last_mut())
                    .ok_or(T1PatchError::InvalidSyntax)?;
                if line.no_newline {
                    return Err(T1PatchError::InvalidSyntax);
                }
                line.no_newline = true;
            } else {
                let (kind, text) = match raw.as_bytes().first() {
                    Some(b' ') => (LineKind::Context, &raw[1..]),
                    Some(b'+') => (LineKind::Add, &raw[1..]),
                    Some(b'-') => (LineKind::Remove, &raw[1..]),
                    _ => return Err(T1PatchError::InvalidSyntax),
                };
                current_hunk
                    .as_mut()
                    .ok_or(T1PatchError::InvalidSyntax)?
                    .0
                    .push(PatchLine {
                        kind,
                        text: text.to_owned(),
                        no_newline: false,
                    });
            }
        }
    }
    finish_hunk(&mut targets, &mut current_hunk)?;
    if targets.is_empty() || targets.iter().any(|target| target.hunks.is_empty()) {
        return Err(T1PatchError::InvalidSyntax);
    }
    Ok(targets)
}

fn unified_path(header: &str) -> Result<&str, T1PatchError> {
    let path = header.split_once('\t').map_or(header, |(path, _)| path);
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .ok_or(T1PatchError::InvalidSyntax)?;
    if path.is_empty() || path == "." || path == "/dev/null" {
        return Err(T1PatchError::InvalidSyntax);
    }
    Ok(path)
}

fn finish_hunk(targets: &mut [Target], current: &mut Option<Hunk>) -> Result<(), T1PatchError> {
    let Some(hunk) = current.take() else {
        return Ok(());
    };
    let valid = !hunk.0.is_empty()
        && hunk.0.iter().any(|line| line.kind != LineKind::Add)
        && hunk.0.iter().any(|line| line.kind != LineKind::Context);
    if !valid {
        return Err(T1PatchError::InvalidSyntax);
    }
    targets
        .last_mut()
        .ok_or(T1PatchError::InvalidSyntax)?
        .hunks
        .push(hunk);
    Ok(())
}
