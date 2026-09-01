use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use serde::de::DeserializeOwned;
use uuid::Uuid;

#[path = "file_io.rs"]
mod file_io;

use super::{PendingCommand, Preferences, PromptHistoryEntry};
use file_io::{
    atomic_write, cleanup_abandoned_temps, create_private_directory, default_root, history_bytes,
    private_append, quarantine, read_history_file, rotate_diagnostics, session_key, sync_directory,
    validate_private_file_if_present, with_lock, MAX_FILE_BYTES,
};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use file_io::{set_injected_fault, FaultPoint};

const MAX_DIAGNOSTIC_BYTES: u64 = 1_024 * 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticEvent {
    Started,
    HostFailure { safe_code: &'static str },
    RetryQueued,
    RetrySent,
    TerminalRestored,
}

impl DiagnosticEvent {
    fn wire(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Started => ("tui_started", None),
            Self::HostFailure { safe_code } => ("host_failure", Some(safe_code)),
            Self::RetryQueued => ("retry_queued", None),
            Self::RetrySent => ("retry_sent", None),
            Self::TerminalRestored => ("terminal_restored", None),
        }
    }
}

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
        create_private_directory(&root.join("diagnostics"))?;
        cleanup_abandoned_temps(&root)?;
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
        if matches!(fs::metadata(&path), Err(error) if error.kind() == io::ErrorKind::NotFound) {
            return Ok(Vec::new());
        }
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

    pub(crate) fn record_diagnostic(&self, event: DiagnosticEvent) -> Result<(), StateError> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        let (kind, safe_code) = event.wire();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| StateError::Unavailable)?
            .as_secs();
        let mut value = serde_json::Map::new();
        value.insert("schema_version".into(), 1.into());
        value.insert("timestamp_unix".into(), timestamp.into());
        value.insert("build_version".into(), env!("CARGO_PKG_VERSION").into());
        value.insert("event".into(), kind.into());
        value.insert("trace_id".into(), Uuid::new_v4().to_string().into());
        if let Some(code) = safe_code {
            value.insert("safe_code".into(), code.into());
        }
        let mut line = serde_json::to_vec(&value).map_err(|_| StateError::Unavailable)?;
        line.push(b'\n');
        with_lock(root, "diagnostics.lock", || {
            let directory = root.join("diagnostics");
            let active = directory.join("garive-tui.log");
            let length = fs::metadata(&active).map_or(0, |metadata| metadata.len());
            if length.saturating_add(line.len() as u64) > MAX_DIAGNOSTIC_BYTES {
                rotate_diagnostics(&directory)?;
            }
            let mut file = private_append(&active)?;
            file.write_all(&line)
                .and_then(|_| file.sync_data())
                .map_err(|_| StateError::Unavailable)
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
