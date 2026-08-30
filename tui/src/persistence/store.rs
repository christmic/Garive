use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use fs2::FileExt;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{PendingCommand, Preferences, PromptHistoryEntry};

const MAX_FILE_BYTES: u64 = 2 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateError {
    Unavailable,
    UnsafePermissions,
    InvalidData,
    Conflict,
}

#[derive(Clone, Debug)]
pub(crate) struct StateStore {
    root: Option<PathBuf>,
}

impl StateStore {
    pub(crate) fn is_ephemeral(&self) -> bool {
        self.root.is_none()
    }

    pub(crate) fn open(
        override_root: Option<PathBuf>,
        ephemeral: bool,
    ) -> Result<Self, StateError> {
        if ephemeral {
            return Ok(Self { root: None });
        }
        let root = override_root
            .or_else(default_root)
            .ok_or(StateError::Unavailable)?;
        create_private_directory(&root)?;
        create_private_directory(&root.join("pending"))?;
        create_private_directory(&root.join("quarantine"))?;
        Ok(Self { root: Some(root) })
    }

    pub(crate) fn load_preferences(&self) -> Result<Preferences, StateError> {
        let Some(root) = &self.root else {
            return Ok(Preferences::default());
        };
        match self.read_json::<Preferences>("preferences.v1.json") {
            Ok(Some(value)) if value.validate().is_ok() => Ok(value),
            Ok(None) => Ok(Preferences::default()),
            Ok(Some(_)) | Err(StateError::InvalidData) => {
                let path = root.join("preferences.v1.json");
                with_lock(root, "state.lock", || {
                    quarantine(root, &path, "preferences")
                })?;
                Ok(Preferences::default())
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn save_preferences(&self, value: &mut Preferences) -> Result<(), StateError> {
        value.validate()?;
        let Some(root) = &self.root else {
            return Ok(());
        };
        with_lock(root, "state.lock", || {
            let current = self.read_json::<Preferences>("preferences.v1.json")?;
            if let Some(current) = &current {
                current.validate()?;
                if current.revision != value.revision {
                    return Err(StateError::Conflict);
                }
            } else if value.revision != 0 {
                return Err(StateError::Conflict);
            }
            let mut next = value.clone();
            next.revision = next.revision.checked_add(1).ok_or(StateError::Conflict)?;
            let bytes = serde_jcs::to_vec(&next).map_err(|_| StateError::InvalidData)?;
            atomic_write(root, "preferences.v1.json", &bytes)?;
            *value = next;
            Ok(())
        })
    }

    pub(crate) fn save_preferences_merged(
        &self,
        value: &mut Preferences,
        base: &mut Preferences,
    ) -> Result<(), StateError> {
        for _ in 0..3 {
            match self.save_preferences(value) {
                Ok(()) => {
                    *base = value.clone();
                    return Ok(());
                }
                Err(StateError::Conflict) => {
                    let current = self.load_preferences()?;
                    *value = Preferences::merge(base, value, &current)?;
                    *base = current;
                }
                Err(error) => return Err(error),
            }
        }
        Err(StateError::Conflict)
    }

    pub(crate) fn save_pending(&self, value: &PendingCommand) -> Result<(), StateError> {
        value.validate()?;
        let Some(root) = &self.root else {
            return Ok(());
        };
        let path = format!(
            "pending/{}.v1.json",
            session_key(value.session_id.as_deref().unwrap_or("new"))
        );
        with_lock(root, "pending.lock", || {
            if let Some(existing) = self.read_json::<PendingCommand>(&path)? {
                existing.validate()?;
                if existing.request_digest != value.request_digest {
                    return Err(StateError::Conflict);
                }
                return Ok(());
            }
            let bytes = serde_jcs::to_vec(value).map_err(|_| StateError::InvalidData)?;
            atomic_write(root, &path, &bytes)
        })
    }

    pub(crate) fn load_pending(&self) -> Result<(Vec<PendingCommand>, usize), StateError> {
        let Some(root) = &self.root else {
            return Ok((Vec::new(), 0));
        };
        with_lock(root, "pending.lock", || {
            let mut entries = fs::read_dir(root.join("pending"))
                .map_err(|_| StateError::Unavailable)?
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().ends_with(".v1.json"))
                .collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.file_name());
            let mut commands = Vec::with_capacity(entries.len());
            let mut quarantined = 0;
            for entry in entries {
                validate_private_file_if_present(&entry.path())?;
                let bytes = fs::read(entry.path()).map_err(|_| StateError::Unavailable)?;
                let decoded = if bytes.len() as u64 > MAX_FILE_BYTES {
                    Err(StateError::InvalidData)
                } else {
                    serde_json::from_slice::<PendingCommand>(&bytes)
                        .map_err(|_| StateError::InvalidData)
                        .and_then(|value| {
                            value.validate()?;
                            Ok(value)
                        })
                };
                match decoded {
                    Ok(value) => commands.push(value),
                    Err(StateError::InvalidData) => {
                        quarantine(root, &entry.path(), "pending")?;
                        quarantined += 1;
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok((commands, quarantined))
        })
    }

    pub(crate) fn remove_pending(&self, session_id: Option<&str>) -> Result<(), StateError> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        let path = root.join(format!(
            "pending/{}.v1.json",
            session_key(session_id.unwrap_or("new"))
        ));
        with_lock(root, "pending.lock", || match fs::remove_file(path) {
            Ok(()) => sync_directory(root),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(StateError::Unavailable),
        })
    }

    pub(crate) fn load_history(&self) -> Result<Vec<PromptHistoryEntry>, StateError> {
        let Some(root) = &self.root else {
            return Ok(Vec::new());
        };
        let path = root.join("prompt-history.v1.jsonl");
        validate_private_file_if_present(&path)?;
        match read_history_file(&path) {
            Ok(value) => Ok(value),
            Err(StateError::InvalidData) => {
                quarantine(root, &path, "prompt-history")?;
                Err(StateError::InvalidData)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn append_history(&self, entry: &PromptHistoryEntry) -> Result<(), StateError> {
        entry.validate()?;
        let Some(root) = &self.root else {
            return Ok(());
        };
        with_lock(root, "history.lock", || {
            let path = root.join("prompt-history.v1.jsonl");
            validate_private_file_if_present(&path)?;
            let mut entries = match fs::metadata(&path) {
                Ok(_) => read_history_file(&path)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(_) => return Err(StateError::Unavailable),
            };
            if entries.last().is_some_and(|last| {
                last.session_id == entry.session_id && last.submitted_text == entry.submitted_text
            }) {
                entries.pop();
            }
            entries.push(entry.clone());
            if entries.len() > 500 {
                entries.drain(..entries.len() - 500);
            }
            let mut bytes = history_bytes(&entries)?;
            while bytes.len() as u64 > MAX_FILE_BYTES && !entries.is_empty() {
                entries.remove(0);
                bytes = history_bytes(&entries)?;
            }
            atomic_write(root, "prompt-history.v1.jsonl", &bytes)
        })
    }

    fn read_json<T: DeserializeOwned>(&self, relative: &str) -> Result<Option<T>, StateError> {
        let Some(root) = &self.root else {
            return Ok(None);
        };
        let path = root.join(relative);
        validate_private_file_if_present(&path)?;
        let bytes = match fs::read(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StateError::Unavailable),
        };
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(StateError::InvalidData);
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| StateError::InvalidData)
    }
}

fn atomic_write(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), StateError> {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        create_private_directory(parent)?;
    }
    validate_private_file_if_present(&destination)?;
    let temporary = destination.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut file = private_create_new(&temporary)?;
    let result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| StateError::Unavailable)
        .and_then(|_| fs::rename(&temporary, &destination).map_err(|_| StateError::Unavailable))
        .and_then(|_| sync_directory(destination.parent().unwrap_or(root)));
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn read_history_file(path: &Path) -> Result<Vec<PromptHistoryEntry>, StateError> {
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

fn history_bytes(entries: &[PromptHistoryEntry]) -> Result<Vec<u8>, StateError> {
    let mut output = Vec::new();
    for entry in entries {
        output.extend(serde_jcs::to_vec(entry).map_err(|_| StateError::InvalidData)?);
        output.push(b'\n');
    }
    Ok(output)
}

fn quarantine(root: &Path, path: &Path, category: &str) -> Result<(), StateError> {
    let target = root
        .join("quarantine")
        .join(format!("{category}-{}.json", Uuid::new_v4()));
    fs::rename(path, target).map_err(|_| StateError::Unavailable)?;
    sync_directory(&root.join("quarantine"))
}

fn with_lock<T>(
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

fn default_root() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|base| base.join("garive/tui"))
}

fn session_key(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> Result<(), StateError> {
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
fn validate_private_file_if_present(path: &Path) -> Result<(), StateError> {
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
#[cfg(not(unix))]
compile_error!("secure Garive TUI local state is currently implemented for Unix targets only");

fn sync_directory(path: &Path) -> Result<(), StateError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| StateError::Unavailable)
}
