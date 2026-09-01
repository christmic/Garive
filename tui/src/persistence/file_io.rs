use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::fs::{File, OpenOptions};

use fs2::FileExt;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{PromptHistoryEntry, StateError};

#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "file_io_windows.rs"]
mod windows;

pub(super) const MAX_FILE_BYTES: u64 = 2 * 1_024 * 1_024;
const DIAGNOSTIC_GENERATIONS: usize = 4;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FaultPoint {
    Write,
    FileSync,
    Rename,
    DirectorySync,
    QuarantineRename,
    QuarantineDirectorySync,
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn set_injected_fault(point: FaultPoint) {
    INJECTED_FAULT.set(Some(point));
}

#[cfg(test)]
thread_local! {
    static INJECTED_FAULT: std::cell::Cell<Option<FaultPoint>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
fn inject(point: FaultPoint) -> Result<(), StateError> {
    if INJECTED_FAULT.get() == Some(point) {
        INJECTED_FAULT.set(None);
        return Err(StateError::Unavailable);
    }
    Ok(())
}

#[cfg(not(test))]
fn inject(_point: ()) -> Result<(), StateError> {
    Ok(())
}

pub(super) fn atomic_write(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), StateError> {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        create_private_directory(parent)?;
    }
    validate_private_file_if_present(&destination)?;
    let temporary = destination.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut file = private_create_new(&temporary)?;
    let result = inject_write()
        .and_then(|_| file.write_all(bytes).map_err(|_| StateError::Unavailable))
        .and_then(|_| inject_file_sync())
        .and_then(|_| file.sync_all().map_err(|_| StateError::Unavailable))
        .and_then(|_| inject_rename())
        .and_then(|_| durable_rename(&temporary, &destination, true))
        .and_then(|_| inject_directory_sync())
        .and_then(|_| sync_directory(destination.parent().unwrap_or(root)));
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) fn read_history_file(path: &Path) -> Result<Vec<PromptHistoryEntry>, StateError> {
    let bytes = fs::read(path).map_err(|_| StateError::Unavailable)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(StateError::InvalidData);
    }
    let complete = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(&[][..], |index| &bytes[..index]);
    if complete.is_empty() {
        return Ok(Vec::new());
    }
    complete
        .split(|byte| *byte == b'\n')
        .map(|line| {
            let value: PromptHistoryEntry =
                serde_json::from_slice(line).map_err(|_| StateError::InvalidData)?;
            value.validate()?;
            Ok(value)
        })
        .collect()
}

pub(super) fn history_bytes(entries: &[PromptHistoryEntry]) -> Result<Vec<u8>, StateError> {
    let mut output = Vec::new();
    for entry in entries {
        output.extend(serde_jcs::to_vec(entry).map_err(|_| StateError::InvalidData)?);
        output.push(b'\n');
    }
    Ok(output)
}

pub(super) fn quarantine(root: &Path, path: &Path, category: &str) -> Result<(), StateError> {
    let target = root
        .join("quarantine")
        .join(format!("{category}-{}.json", Uuid::new_v4()));
    inject_quarantine_rename()?;
    durable_rename(path, &target, false)?;
    inject_quarantine_directory_sync()?;
    sync_directory(&root.join("quarantine"))
}

#[cfg(test)]
fn inject_write() -> Result<(), StateError> {
    inject(FaultPoint::Write)
}

#[cfg(not(test))]
fn inject_write() -> Result<(), StateError> {
    inject(())
}

#[cfg(test)]
fn inject_file_sync() -> Result<(), StateError> {
    inject(FaultPoint::FileSync)
}

#[cfg(not(test))]
fn inject_file_sync() -> Result<(), StateError> {
    inject(())
}

#[cfg(test)]
fn inject_rename() -> Result<(), StateError> {
    inject(FaultPoint::Rename)
}

#[cfg(not(test))]
fn inject_rename() -> Result<(), StateError> {
    inject(())
}

#[cfg(test)]
fn inject_directory_sync() -> Result<(), StateError> {
    inject(FaultPoint::DirectorySync)
}

#[cfg(not(test))]
fn inject_directory_sync() -> Result<(), StateError> {
    inject(())
}

#[cfg(test)]
fn inject_quarantine_rename() -> Result<(), StateError> {
    inject(FaultPoint::QuarantineRename)
}

#[cfg(not(test))]
fn inject_quarantine_rename() -> Result<(), StateError> {
    inject(())
}

#[cfg(test)]
fn inject_quarantine_directory_sync() -> Result<(), StateError> {
    inject(FaultPoint::QuarantineDirectorySync)
}

#[cfg(not(test))]
fn inject_quarantine_directory_sync() -> Result<(), StateError> {
    inject(())
}

pub(super) fn with_lock<T>(
    root: &Path,
    name: &str,
    operation: impl FnOnce() -> Result<T, StateError>,
) -> Result<T, StateError> {
    let lock = private_open(&root.join(name))?;
    lock.lock_exclusive().map_err(|_| StateError::Unavailable)?;
    let result = operation();
    let unlock = FileExt::unlock(&lock).map_err(|_| StateError::Unavailable);
    match (result, unlock) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub(super) fn cleanup_abandoned_temps(root: &Path) -> Result<(), StateError> {
    with_lock(root, "state.lock", || {
        with_lock(root, "history.lock", || {
            with_lock(root, "pending.lock", || {
                cleanup_temp_directory(root, false)?;
                cleanup_temp_directory(&root.join("pending"), true)
            })
        })
    })
}

fn cleanup_temp_directory(directory: &Path, pending: bool) -> Result<(), StateError> {
    let entries = fs::read_dir(directory).map_err(|_| StateError::Unavailable)?;
    let mut removed = false;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_owned_temporary_name(name, pending) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| StateError::Unavailable)?;
        if !metadata.file_type().is_file() {
            continue;
        }
        fs::remove_file(entry.path()).map_err(|_| StateError::Unavailable)?;
        removed = true;
    }
    if removed {
        sync_directory(directory)?;
    }
    Ok(())
}

fn is_owned_temporary_name(name: &str, pending: bool) -> bool {
    let Some((stem, suffix)) = name.rsplit_once(".tmp-") else {
        return false;
    };
    if Uuid::parse_str(suffix).is_err() {
        return false;
    }
    if !pending {
        return matches!(stem, "preferences.v1" | "prompt-history.v1");
    }
    let Some(key) = stem.strip_suffix(".v1") else {
        return false;
    };
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(super) fn rotate_diagnostics(directory: &Path) -> Result<(), StateError> {
    let oldest = directory.join(format!("garive-tui.log.{DIAGNOSTIC_GENERATIONS}"));
    match fs::remove_file(oldest) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(StateError::Unavailable),
    }
    for generation in (1..DIAGNOSTIC_GENERATIONS).rev() {
        let source = directory.join(format!("garive-tui.log.{generation}"));
        let target = directory.join(format!("garive-tui.log.{}", generation + 1));
        match durable_rename(&source, &target, false) {
            Ok(()) => {}
            Err(StateError::Unavailable) if !source.exists() => {}
            Err(error) => return Err(error),
        }
    }
    let active = directory.join("garive-tui.log");
    let first = directory.join("garive-tui.log.1");
    match durable_rename(&active, &first, false) {
        Ok(()) => sync_directory(directory),
        Err(StateError::Unavailable) if !active.exists() => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(windows))]
pub(super) fn default_root() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|base| base.join("garive/tui"))
}

#[cfg(windows)]
pub(super) fn default_root() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|base| base.join("Garive").join("tui"))
}

pub(super) fn session_key(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(unix)]
pub(super) fn create_private_directory(path: &Path) -> Result<(), StateError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(path).map_err(|_| StateError::Unavailable)?;
    if fs::metadata(path)
        .map_err(|_| StateError::Unavailable)?
        .permissions()
        .mode()
        & 0o077
        != 0
    {
        return Err(StateError::UnsafePermissions);
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn validate_private_file_if_present(path: &Path) -> Result<(), StateError> {
    use std::os::unix::fs::PermissionsExt;
    match fs::metadata(path) {
        Ok(value) if value.permissions().mode() & 0o077 != 0 => Err(StateError::UnsafePermissions),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(StateError::Unavailable),
    }
}

#[cfg(unix)]
fn private_create_new(path: &Path) -> Result<File, StateError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| StateError::Unavailable)
}

#[cfg(unix)]
fn private_open(path: &Path) -> Result<File, StateError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|_| StateError::Unavailable)
}

#[cfg(unix)]
pub(super) fn private_append(path: &Path) -> Result<File, StateError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| StateError::Unavailable)
}

#[cfg(windows)]
pub(super) use windows::{
    create_private_directory, private_append, private_create_new, private_open,
    validate_private_file_if_present,
};

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> Result<(), StateError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| StateError::Unavailable)
}

#[cfg(windows)]
pub(super) fn sync_directory(_path: &Path) -> Result<(), StateError> {
    // Windows metadata moves use MOVEFILE_WRITE_THROUGH in durable_rename.
    Ok(())
}

#[cfg(unix)]
fn durable_rename(source: &Path, target: &Path, _replace: bool) -> Result<(), StateError> {
    fs::rename(source, target).map_err(|_| StateError::Unavailable)
}

#[cfg(windows)]
fn durable_rename(source: &Path, target: &Path, replace: bool) -> Result<(), StateError> {
    windows::durable_rename(source, target, replace)
}
