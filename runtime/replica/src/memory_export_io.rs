//! Recoverable native filesystem commit for M2 snapshot packages.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use garive_memory::{project_memory_snapshot, MemorySnapshot, MemorySnapshotLimits};
use serde::{Deserialize, Serialize};

use crate::{
    memory_control::canonical_digest, memory_export::export_binding_digest, MemoryControlGrant,
    MemoryControlRuntimeError, MemoryExportCommand, MemoryExportReceipt, MemoryExportTarget,
    SqliteLedger,
};

/// Exports one authorized fixed-revision snapshot and durably journals its public receipt.
pub fn export_memory_snapshot(
    ledger: &mut SqliteLedger,
    grant: &MemoryControlGrant,
    command: &MemoryExportCommand,
    target: &MemoryExportTarget,
    limits: MemorySnapshotLimits,
) -> Result<MemoryExportReceipt, MemoryControlRuntimeError> {
    let projection =
        ledger.read_memory_control_projection(grant, command.namespace_id(), limits.document)?;
    let snapshot = project_memory_snapshot(
        command.export_id(),
        command.namespace_id(),
        projection.repository_revision,
        command.exported_at(),
        projection.documents,
    )
    .map_err(MemoryControlRuntimeError::from)?;
    enforce_bounds(&snapshot, limits)?;
    let entry_count = u64::try_from(snapshot.documents.len())
        .map_err(|_| MemoryControlRuntimeError::BoundExceeded)?;
    let (receipt, receipt_json) = MemoryExportReceipt::create(
        command,
        &snapshot.manifest.manifest_digest,
        projection.repository_revision,
        entry_count,
    )?;
    let binding = export_binding_digest(command, target, &snapshot.manifest.manifest_digest)?;
    let recovery = RecoveryRecord {
        schema_version: 1,
        binding_digest: &binding,
        manifest_digest: &snapshot.manifest.manifest_digest,
        receipt_json: &receipt_json,
    };
    let (recovery_json, _) = canonical_digest(&recovery)?;
    let paths = ExportPaths::new(target.path(), &binding)?;
    if let Some(committed) = ledger.read_memory_export_journal(command, target, &receipt)? {
        verify_package(&paths.final_path, &snapshot)?;
        if fs::symlink_metadata(&paths.stage).is_ok() {
            return Err(MemoryControlRuntimeError::ExportTargetInvalid);
        }
        if fs::symlink_metadata(&paths.journal).is_ok() {
            ensure_recovery_record(&paths, &recovery_json)?;
            fs::remove_file(&paths.journal).map_err(persistence)?;
            sync_directory(&paths.parent)?;
        }
        return Ok(committed);
    }
    ensure_recovery_record(&paths, &recovery_json)?;
    ensure_destination(&paths, &snapshot)?;
    let committed = ledger.commit_memory_export_journal(grant, command, target, &receipt)?;
    fs::remove_file(&paths.journal).map_err(persistence)?;
    sync_directory(&paths.parent)?;
    Ok(committed)
}

#[derive(Serialize)]
struct RecoveryRecord<'a> {
    schema_version: u8,
    binding_digest: &'a str,
    manifest_digest: &'a str,
    receipt_json: &'a str,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredRecoveryRecord {
    schema_version: u8,
    binding_digest: String,
    manifest_digest: String,
    receipt_json: String,
}

struct ExportPaths {
    parent: PathBuf,
    final_path: PathBuf,
    stage: PathBuf,
    journal: PathBuf,
}

impl ExportPaths {
    fn new(final_path: &Path, binding: &str) -> Result<Self, MemoryControlRuntimeError> {
        let parent = final_path
            .parent()
            .ok_or(MemoryControlRuntimeError::ExportTargetInvalid)?
            .to_owned();
        let metadata = fs::symlink_metadata(&parent)
            .map_err(|_| MemoryControlRuntimeError::ExportTargetInvalid)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(MemoryControlRuntimeError::ExportTargetInvalid);
        }
        let tag = &binding[..32];
        Ok(Self {
            parent: parent.clone(),
            final_path: final_path.to_owned(),
            stage: parent.join(format!(".garive-memory-{tag}.stage")),
            journal: parent.join(format!(".garive-memory-{tag}.journal")),
        })
    }
}

fn enforce_bounds(
    snapshot: &MemorySnapshot,
    limits: MemorySnapshotLimits,
) -> Result<(), MemoryControlRuntimeError> {
    if snapshot.documents.len() > limits.max_entries {
        return Err(MemoryControlRuntimeError::BoundExceeded);
    }
    let total = snapshot
        .documents
        .iter()
        .try_fold(snapshot.manifest_json.len(), |total, (_, document)| {
            total.checked_add(document.render().len())
        })
        .ok_or(MemoryControlRuntimeError::BoundExceeded)?;
    if total > limits.max_total_bytes {
        Err(MemoryControlRuntimeError::BoundExceeded)
    } else {
        Ok(())
    }
}

fn ensure_recovery_record(
    paths: &ExportPaths,
    expected: &str,
) -> Result<(), MemoryControlRuntimeError> {
    match fs::symlink_metadata(&paths.journal) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(MemoryControlRuntimeError::ExportTargetInvalid);
            }
            let bytes = read_bounded(&paths.journal, 16 * 1024)?;
            let json = String::from_utf8(bytes)
                .map_err(|_| MemoryControlRuntimeError::ExportTargetInvalid)?;
            let stored: StoredRecoveryRecord = serde_json::from_str(&json)
                .map_err(|_| MemoryControlRuntimeError::ExportTargetInvalid)?;
            let (canonical, _) = canonical_digest(&stored)?;
            if stored.schema_version != 1 || canonical != json || json != expected {
                return Err(MemoryControlRuntimeError::ExportTargetInvalid);
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if fs::symlink_metadata(&paths.final_path).is_ok()
                || fs::symlink_metadata(&paths.stage).is_ok()
            {
                return Err(MemoryControlRuntimeError::ExportTargetInvalid);
            }
            write_new(&paths.journal, expected.as_bytes())?;
            sync_directory(&paths.parent)
        }
        Err(_) => Err(MemoryControlRuntimeError::ExportTargetInvalid),
    }
}

fn ensure_destination(
    paths: &ExportPaths,
    snapshot: &MemorySnapshot,
) -> Result<(), MemoryControlRuntimeError> {
    if fs::symlink_metadata(&paths.final_path).is_ok() {
        return verify_package(&paths.final_path, snapshot);
    }
    if fs::symlink_metadata(&paths.stage).is_ok() {
        verify_package(&paths.stage, snapshot)?;
    } else {
        write_package(&paths.stage, snapshot)?;
    }
    fs::rename(&paths.stage, &paths.final_path).map_err(persistence)?;
    sync_directory(&paths.parent)
}

fn write_package(stage: &Path, snapshot: &MemorySnapshot) -> Result<(), MemoryControlRuntimeError> {
    fs::create_dir(stage).map_err(persistence)?;
    let entries = stage.join("entries");
    fs::create_dir(&entries).map_err(persistence)?;
    for (relative, document) in &snapshot.documents {
        let name = relative
            .strip_prefix("entries/")
            .ok_or(MemoryControlRuntimeError::InvalidSnapshot)?;
        write_new(&entries.join(name), document.render().as_bytes())?;
    }
    sync_directory(&entries)?;
    write_new(&stage.join("manifest.json"), &snapshot.manifest_json)?;
    sync_directory(stage)
}

fn verify_package(root: &Path, snapshot: &MemorySnapshot) -> Result<(), MemoryControlRuntimeError> {
    require_directory(root)?;
    let entries = root.join("entries");
    require_directory(&entries)?;
    let root_names = directory_names(root)?;
    if root_names != ["entries".to_owned(), "manifest.json".to_owned()].into() {
        return Err(MemoryControlRuntimeError::ExportTargetInvalid);
    }
    if read_bounded(&root.join("manifest.json"), snapshot.manifest_json.len())?
        != snapshot.manifest_json
    {
        return Err(MemoryControlRuntimeError::ExportTargetInvalid);
    }
    let expected = snapshot
        .documents
        .iter()
        .map(|(path, document)| {
            (
                path.trim_start_matches("entries/").to_owned(),
                document.render().into_bytes(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if directory_names(&entries)? != expected.keys().cloned().collect() {
        return Err(MemoryControlRuntimeError::ExportTargetInvalid);
    }
    for (name, bytes) in expected {
        if read_bounded(&entries.join(name), bytes.len())? != bytes {
            return Err(MemoryControlRuntimeError::ExportTargetInvalid);
        }
    }
    Ok(())
}

fn directory_names(
    path: &Path,
) -> Result<std::collections::BTreeSet<String>, MemoryControlRuntimeError> {
    fs::read_dir(path)
        .map_err(persistence)?
        .map(|entry| {
            entry
                .map_err(persistence)?
                .file_name()
                .into_string()
                .map_err(|_| MemoryControlRuntimeError::ExportTargetInvalid)
        })
        .collect()
}

fn require_directory(path: &Path) -> Result<(), MemoryControlRuntimeError> {
    let metadata = fs::symlink_metadata(path).map_err(persistence)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(MemoryControlRuntimeError::ExportTargetInvalid)
    }
}

fn read_bounded(path: &Path, expected: usize) -> Result<Vec<u8>, MemoryControlRuntimeError> {
    let metadata = fs::symlink_metadata(path).map_err(persistence)?;
    #[cfg(unix)]
    let one_link = metadata.nlink() == 1;
    #[cfg(not(unix))]
    let one_link = true;
    if !metadata.is_file() || metadata.file_type().is_symlink() || !one_link {
        return Err(MemoryControlRuntimeError::ExportTargetInvalid);
    }
    let limit = u64::try_from(expected)
        .map_err(|_| MemoryControlRuntimeError::BoundExceeded)?
        .saturating_add(1);
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(persistence)?
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(persistence)?;
    if bytes.len() > expected {
        return Err(MemoryControlRuntimeError::ExportTargetInvalid);
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), MemoryControlRuntimeError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(persistence)?;
    file.write_all(bytes).map_err(persistence)?;
    file.sync_all().map_err(persistence)
}

fn sync_directory(path: &Path) -> Result<(), MemoryControlRuntimeError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(persistence)
}

fn persistence(_: std::io::Error) -> MemoryControlRuntimeError {
    MemoryControlRuntimeError::PersistenceFailed
}
