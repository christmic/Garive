//! Descriptor-rooted T1 workspace read and list executor.

use std::{collections::BinaryHeap, fs::File, io::Read, path::Path};

use garive_ledger::CanonicalPayload;
use garive_tools::{
    AccessMode, AccessNamespace, BuiltinT1Catalogue, EffectReceipt, ExecutionCapability,
    ExecutionFact, InvocationGrant, PreparedToolCall, ReceiptId, ReplayClass,
    TerminalClassification, ToolIntent, ToolInvocationId, T1_LIST, T1_READ_TEXT, T1_SEARCH_TEXT,
};
use rustix::{
    fd::OwnedFd,
    fs::{fstat, open, openat, statat, AtFlags, Dir, FileType, Mode, OFlags},
    io::{dup, Errno},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, PreparedExecution,
};

/// Stable executor identity used by matching F0 sandbox bindings.
pub const T1_WORKSPACE_EXECUTOR_ID: &str = "garive.builtin.workspace";

/// Unix descriptor-confined implementation of T1 read and list.
pub struct BuiltinWorkspaceExecutor {
    root: OwnedFd,
    revision: String,
    catalogue: BuiltinT1Catalogue,
}

impl BuiltinWorkspaceExecutor {
    /// Opens one explicit workspace directory without following a root link.
    pub fn new(
        workspace_root: impl AsRef<Path>,
        revision: impl Into<String>,
        catalogue: BuiltinT1Catalogue,
    ) -> Result<Self, std::io::Error> {
        let revision = revision.into();
        if revision.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "empty T1 workspace executor revision",
            ));
        }
        let root = open(
            workspace_root.as_ref(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Self {
            root,
            revision,
            catalogue,
        })
    }
}

impl ExecutorPort for BuiltinWorkspaceExecutor {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        operation(&self.catalogue, invocation_id, prepared, grant)?;
        fstat(&self.root).map_err(|_| "workspace capability unavailable".to_owned())?;
        Ok(PreparedExecution {
            executor_id: T1_WORKSPACE_EXECUTOR_ID.into(),
            executor_revision: self.revision.clone(),
            dispatch_attempt_id: format!(
                "dispatch-{:x}",
                Sha256::digest(invocation_id.as_str().as_bytes())
            ),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        let operation = operation(
            &self.catalogue,
            command.invocation_id,
            command.prepared,
            command.grant,
        );
        let root = dup(&self.root);
        let expected_attempt = format!(
            "dispatch-{:x}",
            Sha256::digest(command.invocation_id.as_str().as_bytes())
        );
        Box::pin(async move {
            if command.execution.executor_id != T1_WORKSPACE_EXECUTOR_ID
                || command.execution.executor_revision != self.revision
                || command.execution.dispatch_attempt_id != expected_attempt
            {
                return Err(ExecutorDispatchError::ReceiptInvalid);
            }
            let operation = operation.map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
            let root = root.map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            let result = tokio::task::spawn_blocking(move || execute(root, operation))
                .await
                .map_err(|_| ExecutorDispatchError::ExecutorStateUnknown)?;
            match result {
                Ok(content) => completed(&command, content),
                Err(error) => failed(&command, error.code()),
            }
        })
    }
}

enum Operation {
    Read {
        path: String,
        max_bytes: u64,
        result_bound: u64,
    },
    List {
        path: String,
        max_entries: usize,
        include_hidden: bool,
        max_nodes: usize,
        result_bound: u64,
    },
    Search {
        path: String,
        query: String,
        case_sensitive: bool,
        max_matches: usize,
        max_file_bytes: u64,
        max_nodes: usize,
        result_bound: u64,
    },
}

fn operation(
    catalogue: &BuiltinT1Catalogue,
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
) -> Result<Operation, String> {
    if prepared.contract_version() != 3
        || prepared.replay_class() != ReplayClass::ReadOnly
        || grant.invocation_id != *invocation_id
        || grant.prepared_digest != prepared.input_digest()
        || grant.tool_name != prepared.tool_name()
        || grant.tool_revision != prepared.tool_revision()
        || !requirements_cover(prepared, grant)
    {
        return Err("invalid T1 execution binding".into());
    }
    let reconstructed = catalogue
        .prepare(&ToolIntent::new(
            prepared.model_call_id(),
            prepared.tool_name(),
            prepared.normalized_arguments(),
        ))
        .map_err(|_| "T1 definition mismatch")?;
    if reconstructed != *prepared {
        return Err("T1 definition mismatch".into());
    }
    let [access] = prepared
        .invocation_accesses()
        .ok_or_else(|| "missing T1 access".to_owned())?
        .values()
    else {
        return Err("one T1 workspace access required".into());
    };
    if access.namespace() != AccessNamespace::Filesystem || access.mode() != AccessMode::Read {
        return Err("read-only T1 workspace access required".into());
    }
    let arguments: Value = serde_json::from_str(prepared.normalized_arguments())
        .map_err(|_| "invalid T1 arguments")?;
    let path = arguments
        .get("path")
        .and_then(Value::as_str)
        .filter(|value| *value == access.resource_key())
        .ok_or_else(|| "T1 path/access mismatch".to_owned())?
        .to_owned();
    let result_bound = prepared
        .max_result_bytes()
        .ok_or_else(|| "missing T1 result bound".to_owned())?
        .min(grant.granted_requirements.max_output_bytes());
    match prepared.tool_name() {
        T1_READ_TEXT => Ok(Operation::Read {
            path,
            max_bytes: number(&arguments, "max_bytes")?,
            result_bound,
        }),
        T1_LIST => Ok(Operation::List {
            path,
            max_entries: usize::try_from(number(&arguments, "max_entries")?)
                .map_err(|_| "invalid T1 entry bound")?,
            include_hidden: arguments
                .get("include_hidden")
                .and_then(Value::as_bool)
                .ok_or_else(|| "invalid T1 hidden policy".to_owned())?,
            max_nodes: usize::try_from(number(&arguments, "max_nodes")?)
                .map_err(|_| "invalid T1 node bound")?,
            result_bound,
        }),
        T1_SEARCH_TEXT => Ok(Operation::Search {
            path,
            query: arguments
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| "invalid T1 search query".to_owned())?
                .to_owned(),
            case_sensitive: arguments
                .get("case_sensitive")
                .and_then(Value::as_bool)
                .ok_or_else(|| "invalid T1 search case policy".to_owned())?,
            max_matches: usize::try_from(number(&arguments, "max_matches")?)
                .map_err(|_| "invalid T1 match bound")?,
            max_file_bytes: number(&arguments, "max_file_bytes")?,
            max_nodes: usize::try_from(number(&arguments, "max_nodes")?)
                .map_err(|_| "invalid T1 node bound")?,
            result_bound,
        }),
        _ => Err("unsupported T1 workspace operation".into()),
    }
}

fn requirements_cover(prepared: &PreparedToolCall, grant: &InvocationGrant) -> bool {
    let requested = prepared.requirements();
    let granted = &grant.granted_requirements;
    requested.capabilities().eq(granted.capabilities())
        && granted.max_duration_ms() <= requested.max_duration_ms()
        && granted.max_output_bytes() <= requested.max_output_bytes()
        && requested
            .capabilities()
            .eq([ExecutionCapability::FilesystemRead])
}

fn number(arguments: &Value, name: &str) -> Result<u64, String> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("invalid T1 {name}"))
}

fn execute(root: OwnedFd, operation: Operation) -> Result<Value, WorkspaceExecutionError> {
    match operation {
        Operation::Read {
            path,
            max_bytes,
            result_bound,
        } => read_text(root, &path, max_bytes, result_bound),
        Operation::List {
            path,
            max_entries,
            include_hidden,
            max_nodes,
            result_bound,
        } => list(
            root,
            &path,
            max_entries,
            include_hidden,
            max_nodes,
            result_bound,
        ),
        Operation::Search {
            path,
            query,
            case_sensitive,
            max_matches,
            max_file_bytes,
            max_nodes,
            result_bound,
        } => search(
            root,
            &path,
            &query,
            case_sensitive,
            max_matches,
            max_file_bytes,
            max_nodes,
            result_bound,
        ),
    }
}

fn read_text(
    root: OwnedFd,
    path: &str,
    max_bytes: u64,
    result_bound: u64,
) -> Result<Value, WorkspaceExecutionError> {
    let file = open_target(root, path)?;
    if FileType::from_raw_mode(fstat(&file).map_err(map_errno)?.st_mode) != FileType::RegularFile {
        return Err(WorkspaceExecutionError::PathTypeMismatch);
    }
    let mut bytes = Vec::new();
    File::from(file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(map_io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(WorkspaceExecutionError::ResultBoundExceeded);
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let text = String::from_utf8(bytes).map_err(|_| WorkspaceExecutionError::NonUtf8Content)?;
    bounded_result(
        json!({"path":path,"text":text,"byte_count":text.len(),"content_digest":digest,"truncated":false}),
        result_bound,
        WorkspaceExecutionError::ResultBoundExceeded,
    )
}

fn list(
    root: OwnedFd,
    path: &str,
    max_entries: usize,
    include_hidden: bool,
    max_nodes: usize,
    result_bound: u64,
) -> Result<Value, WorkspaceExecutionError> {
    let directory = open_target(root, path)?;
    if FileType::from_raw_mode(fstat(&directory).map_err(map_errno)?.st_mode) != FileType::Directory
    {
        return Err(WorkspaceExecutionError::PathTypeMismatch);
    }
    let mut stream = Dir::new(directory).map_err(map_errno)?;
    let mut smallest = BinaryHeap::<(String, &'static str)>::new();
    let mut eligible = 0usize;
    let mut nodes = 0usize;
    while let Some(entry) = stream.next() {
        let entry = entry.map_err(map_errno)?;
        let name = entry.file_name().to_bytes();
        if matches!(name, b"." | b"..") {
            continue;
        }
        nodes = nodes.saturating_add(1);
        if nodes > max_nodes {
            return Err(WorkspaceExecutionError::EntryBoundExceeded);
        }
        if !include_hidden && name.first() == Some(&b'.') {
            continue;
        }
        let name = std::str::from_utf8(name)
            .map_err(|_| WorkspaceExecutionError::AccessDenied)?
            .to_owned();
        let stat = statat(
            stream.fd().map_err(map_errno)?,
            entry.file_name(),
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(map_errno)?;
        let kind = match FileType::from_raw_mode(stat.st_mode) {
            FileType::RegularFile => "file",
            FileType::Directory => "directory",
            FileType::Symlink => "symlink",
            _ => "other",
        };
        eligible = eligible.saturating_add(1);
        let candidate = (name, kind);
        if smallest.len() <= max_entries {
            smallest.push(candidate);
        } else if smallest.peek().is_some_and(|largest| candidate < *largest) {
            smallest.pop();
            smallest.push(candidate);
        }
    }
    let mut entries = smallest.into_vec();
    entries.sort();
    entries.truncate(max_entries);
    let entries = entries
        .into_iter()
        .map(|(name, kind)| json!({"name":name,"kind":kind}))
        .collect::<Vec<_>>();
    bounded_result(
        json!({"path":path,"entries":entries,"truncated":eligible > max_entries}),
        result_bound,
        WorkspaceExecutionError::EntryBoundExceeded,
    )
}

#[allow(clippy::too_many_arguments)]
fn search(
    root: OwnedFd,
    path: &str,
    query: &str,
    case_sensitive: bool,
    max_matches: usize,
    max_file_bytes: u64,
    max_nodes: usize,
    result_bound: u64,
) -> Result<Value, WorkspaceExecutionError> {
    let target = open_target(root, path)?;
    let target_type = FileType::from_raw_mode(fstat(&target).map_err(map_errno)?.st_mode);
    let mut state = SearchState::new(
        query,
        case_sensitive,
        max_matches,
        max_file_bytes,
        max_nodes,
    );
    match target_type {
        FileType::RegularFile => state.scan_file(target, path),
        FileType::Directory => state.walk_directory(target, path)?,
        _ => return Err(WorkspaceExecutionError::PathTypeMismatch),
    }
    bounded_result(
        json!({
            "matches":state.matches,
            "files_scanned":state.files_scanned,
            "skipped":{
                "access_denied":state.skipped_access,
                "non_utf8_content":state.skipped_utf8,
                "result_bound_exceeded":state.skipped_bound
            },
            "truncated":state.total_matches > max_matches
        }),
        result_bound,
        WorkspaceExecutionError::SearchBoundExceeded,
    )
}

struct SearchState<'a> {
    query: &'a str,
    case_sensitive: bool,
    max_matches: usize,
    max_file_bytes: u64,
    max_nodes: usize,
    nodes: usize,
    matches: Vec<Value>,
    total_matches: usize,
    files_scanned: usize,
    skipped_access: usize,
    skipped_utf8: usize,
    skipped_bound: usize,
}

impl<'a> SearchState<'a> {
    fn new(
        query: &'a str,
        case_sensitive: bool,
        max_matches: usize,
        max_file_bytes: u64,
        max_nodes: usize,
    ) -> Self {
        Self {
            query,
            case_sensitive,
            max_matches,
            max_file_bytes,
            max_nodes,
            nodes: 0,
            matches: Vec::new(),
            total_matches: 0,
            files_scanned: 0,
            skipped_access: 0,
            skipped_utf8: 0,
            skipped_bound: 0,
        }
    }

    fn walk_directory(
        &mut self,
        directory: OwnedFd,
        path: &str,
    ) -> Result<(), WorkspaceExecutionError> {
        let entries = self.directory_entries(&directory)?;
        let mut stack = vec![SearchFrame {
            directory,
            path: path.to_owned(),
            entries,
            next: 0,
        }];
        loop {
            let Some(frame) = stack.last_mut() else {
                return Ok(());
            };
            let Some(entry) = frame.entries.get(frame.next).cloned() else {
                stack.pop();
                continue;
            };
            frame.next += 1;
            let child_path = if frame.path == "." {
                entry.name.clone()
            } else {
                format!("{}/{}", frame.path, entry.name)
            };
            match entry.kind {
                FileType::Directory => {
                    let child = openat(
                        &frame.directory,
                        entry.name.as_str(),
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(map_errno)?;
                    let entries = self.directory_entries(&child)?;
                    stack.push(SearchFrame {
                        directory: child,
                        path: child_path,
                        entries,
                        next: 0,
                    });
                }
                FileType::RegularFile => {
                    match openat(
                        &frame.directory,
                        entry.name.as_str(),
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    ) {
                        Ok(file) => self.scan_file(file, &child_path),
                        Err(_) => self.skipped_access = self.skipped_access.saturating_add(1),
                    }
                }
                _ => {}
            }
        }
    }

    fn directory_entries(
        &mut self,
        directory: &OwnedFd,
    ) -> Result<Vec<SearchEntry>, WorkspaceExecutionError> {
        let mut stream = Dir::read_from(directory).map_err(map_errno)?;
        let mut output = Vec::new();
        while let Some(entry) = stream.next() {
            let entry = entry.map_err(map_errno)?;
            let name = entry.file_name().to_bytes();
            if matches!(name, b"." | b"..") {
                continue;
            }
            self.nodes = self.nodes.saturating_add(1);
            if self.nodes > self.max_nodes {
                return Err(WorkspaceExecutionError::SearchBoundExceeded);
            }
            let name = std::str::from_utf8(name)
                .map_err(|_| WorkspaceExecutionError::AccessDenied)?
                .to_owned();
            let stat = statat(
                stream.fd().map_err(map_errno)?,
                entry.file_name(),
                AtFlags::SYMLINK_NOFOLLOW,
            )
            .map_err(map_errno)?;
            output.push(SearchEntry {
                name,
                kind: FileType::from_raw_mode(stat.st_mode),
            });
        }
        output.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(output)
    }

    fn scan_file(&mut self, file: OwnedFd, path: &str) {
        let Ok(stat) = fstat(&file) else {
            self.skipped_access = self.skipped_access.saturating_add(1);
            return;
        };
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return;
        }
        let mut bytes = Vec::new();
        if File::from(file)
            .take(self.max_file_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .is_err()
        {
            self.skipped_access = self.skipped_access.saturating_add(1);
            return;
        }
        if bytes.len() as u64 > self.max_file_bytes {
            self.skipped_bound = self.skipped_bound.saturating_add(1);
            return;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            self.skipped_utf8 = self.skipped_utf8.saturating_add(1);
            return;
        };
        self.files_scanned = self.files_scanned.saturating_add(1);
        for (line_index, line) in text.lines().enumerate() {
            for byte_offset in literal_offsets(line, self.query, self.case_sensitive) {
                self.total_matches = self.total_matches.saturating_add(1);
                if self.matches.len() < self.max_matches {
                    let scalar_offset = line[..byte_offset].chars().count();
                    self.matches.push(json!({
                        "path":path,
                        "line":line_index + 1,
                        "column":scalar_offset + 1,
                        "preview":preview(line, scalar_offset)
                    }));
                }
            }
        }
    }
}

struct SearchFrame {
    directory: OwnedFd,
    path: String,
    entries: Vec<SearchEntry>,
    next: usize,
}

#[derive(Clone)]
struct SearchEntry {
    name: String,
    kind: FileType,
}

fn literal_offsets(line: &str, query: &str, case_sensitive: bool) -> Vec<usize> {
    let line_bytes = line.as_bytes();
    let query_bytes = query.as_bytes();
    let mut output = Vec::new();
    let mut offset = 0usize;
    while offset + query_bytes.len() <= line_bytes.len() {
        let end = offset + query_bytes.len();
        let equal = line.is_char_boundary(offset)
            && line.is_char_boundary(end)
            && line_bytes[offset..end]
                .iter()
                .zip(query_bytes)
                .all(|(left, right)| {
                    left == right
                        || (!case_sensitive
                            && left.is_ascii()
                            && right.is_ascii()
                            && left.eq_ignore_ascii_case(right))
                });
        if equal {
            output.push(offset);
            offset = end;
        } else {
            offset += line[offset..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
        }
    }
    output
}

fn preview(line: &str, match_scalar: usize) -> String {
    const LIMIT: usize = 256;
    const BEFORE: usize = 96;
    let scalars = line.chars().collect::<Vec<_>>();
    if scalars.len() <= LIMIT {
        return line.to_owned();
    }
    let start = match_scalar.saturating_sub(BEFORE);
    let end = (start + LIMIT).min(scalars.len());
    let mut output = String::new();
    if start != 0 {
        output.push('…');
    }
    output.extend(&scalars[start..end]);
    if end != scalars.len() {
        output.push('…');
    }
    output
}

fn open_target(root: OwnedFd, path: &str) -> Result<OwnedFd, WorkspaceExecutionError> {
    let mut current = root;
    if path == "." {
        return Ok(current);
    }
    let mut components = path.split('/').peekable();
    while let Some(component) = components.next() {
        if !has_exact_component(&current, component.as_bytes())? {
            return Err(WorkspaceExecutionError::PathNotFound);
        }
        let last = components.peek().is_none();
        let flags = OFlags::RDONLY
            | OFlags::NOFOLLOW
            | OFlags::CLOEXEC
            | if !last {
                OFlags::DIRECTORY
            } else {
                OFlags::empty()
            };
        current = openat(&current, component, flags, Mode::empty()).map_err(map_errno)?;
    }
    Ok(current)
}

fn has_exact_component(
    directory: &OwnedFd,
    component: &[u8],
) -> Result<bool, WorkspaceExecutionError> {
    let entries = Dir::read_from(directory).map_err(map_errno)?;
    for entry in entries {
        if entry.map_err(map_errno)?.file_name().to_bytes() == component {
            return Ok(true);
        }
    }
    Ok(false)
}

fn bounded_result(
    value: Value,
    bound: u64,
    error: WorkspaceExecutionError,
) -> Result<Value, WorkspaceExecutionError> {
    let payload = CanonicalPayload::from_value(&value).map_err(|_| error)?;
    if payload.as_json().len() as u64 > bound {
        Err(error)
    } else {
        Ok(value)
    }
}

#[derive(Clone, Copy)]
enum WorkspaceExecutionError {
    PathNotFound,
    PathTypeMismatch,
    AccessDenied,
    NonUtf8Content,
    ResultBoundExceeded,
    EntryBoundExceeded,
    SearchBoundExceeded,
}

impl WorkspaceExecutionError {
    const fn code(self) -> &'static str {
        match self {
            Self::PathNotFound => "path_not_found",
            Self::PathTypeMismatch => "path_type_mismatch",
            Self::AccessDenied => "access_denied",
            Self::NonUtf8Content => "non_utf8_content",
            Self::ResultBoundExceeded => "result_bound_exceeded",
            Self::EntryBoundExceeded => "entry_bound_exceeded",
            Self::SearchBoundExceeded => "search_bound_exceeded",
        }
    }
}

fn map_errno(error: Errno) -> WorkspaceExecutionError {
    match error {
        Errno::NOENT => WorkspaceExecutionError::PathNotFound,
        Errno::NOTDIR | Errno::ISDIR => WorkspaceExecutionError::PathTypeMismatch,
        _ => WorkspaceExecutionError::AccessDenied,
    }
}

fn map_io(error: std::io::Error) -> WorkspaceExecutionError {
    match error.kind() {
        std::io::ErrorKind::NotFound => WorkspaceExecutionError::PathNotFound,
        std::io::ErrorKind::InvalidData => WorkspaceExecutionError::NonUtf8Content,
        _ => WorkspaceExecutionError::AccessDenied,
    }
}

fn completed(
    command: &ExecutorDispatch<'_>,
    content: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let digest = CanonicalPayload::from_value(&content)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Completed {
        receipt: Some(receipt(command, TerminalClassification::Completed, digest)?),
        content,
        truncated: false,
    })
}

fn failed(
    command: &ExecutorDispatch<'_>,
    code: &str,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let evidence = json!({"code":code,"details":null,"partial":null});
    let digest = CanonicalPayload::from_value(&evidence)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(ExecutionFact::Failed {
        receipt: Some(receipt(command, TerminalClassification::Failed, digest)?),
        code: code.into(),
        details: None,
        partial: None,
    })
}

fn receipt(
    command: &ExecutorDispatch<'_>,
    classification: TerminalClassification,
    result_digest: String,
) -> Result<EffectReceipt, ExecutorDispatchError> {
    Ok(EffectReceipt {
        receipt_id: ReceiptId::new(command.receipt_id)
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
        invocation_id: command.invocation_id.clone(),
        prepared_digest: command.prepared.input_digest().into(),
        grant_id: command.grant.grant_id.clone(),
        executor_id: command.execution.executor_id.clone(),
        executor_revision: command.execution.executor_revision.clone(),
        terminal_classification: classification,
        result_digest,
    })
}
