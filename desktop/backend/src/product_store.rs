use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Maximum encoded bytes accepted for either disposable preferences or pending identity.
pub const MAX_PRODUCT_STORE_BYTES: usize = 64 * 1024;
/// Maximum encoded bytes accepted for the separate update reconciliation record.
pub const MAX_UPDATE_PENDING_BYTES: usize = 256;
const PREFERENCES_FILE: &str = "client-preferences-v1.json";
const PENDING_FILE: &str = "pending-command-v1.json";
const UPDATE_PENDING_FILE: &str = "pending-update-v1.json";

/// Stable content-free failure from the Desktop product-local store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopProductStoreError {
    /// The explicit store directory or requested bytes violate bounds.
    InvalidValue,
    /// The local disposable store could not be read or committed atomically.
    Unavailable,
}

impl DesktopProductStoreError {
    /// Returns the only frontend-safe error code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidValue => "local_preference_invalid",
            Self::Unavailable => "local_preference_unavailable",
        }
    }
}

/// Atomic, bounded byte storage for UX-A preferences and pending command identity.
pub struct DesktopProductStore {
    directory: PathBuf,
    gate: Mutex<()>,
}

impl DesktopProductStore {
    /// Creates a store rooted at an explicit application-owned directory.
    pub fn new(directory: impl Into<PathBuf>) -> Result<Self, DesktopProductStoreError> {
        let directory = directory.into();
        if directory.as_os_str().is_empty() || directory.parent().is_none() {
            return Err(DesktopProductStoreError::InvalidValue);
        }
        Ok(Self {
            directory,
            gate: Mutex::new(()),
        })
    }

    /// Reads the exact preference document, or absence after first launch.
    pub fn read_preferences(&self) -> Result<Option<Vec<u8>>, DesktopProductStoreError> {
        self.read(PREFERENCES_FILE, MAX_PRODUCT_STORE_BYTES)
    }

    /// Atomically replaces the bounded preference document.
    pub fn write_preferences(&self, value: &[u8]) -> Result<(), DesktopProductStoreError> {
        self.write(PREFERENCES_FILE, value, MAX_PRODUCT_STORE_BYTES)
    }

    /// Reads the separately stored exact pending command record.
    pub fn read_pending(&self) -> Result<Option<Vec<u8>>, DesktopProductStoreError> {
        self.read(PENDING_FILE, MAX_PRODUCT_STORE_BYTES)
    }

    /// Atomically replaces or explicitly clears the pending command record.
    pub fn write_pending(&self, value: Option<&[u8]>) -> Result<(), DesktopProductStoreError> {
        self.write_optional(PENDING_FILE, value, MAX_PRODUCT_STORE_BYTES)
    }

    /// Reads the separately stored update reconciliation record.
    pub fn read_update_pending(&self) -> Result<Option<Vec<u8>>, DesktopProductStoreError> {
        self.read(UPDATE_PENDING_FILE, MAX_UPDATE_PENDING_BYTES)
    }

    /// Atomically replaces or clears the bounded update reconciliation record.
    pub fn write_update_pending(
        &self,
        value: Option<&[u8]>,
    ) -> Result<(), DesktopProductStoreError> {
        self.write_optional(UPDATE_PENDING_FILE, value, MAX_UPDATE_PENDING_BYTES)
    }

    fn write_optional(
        &self,
        name: &str,
        value: Option<&[u8]>,
        maximum: usize,
    ) -> Result<(), DesktopProductStoreError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| DesktopProductStoreError::Unavailable)?;
        fs::create_dir_all(&self.directory).map_err(map_io)?;
        let path = self.directory.join(name);
        match value {
            Some(bytes) => write_atomic(&self.directory, &path, bytes, maximum),
            None => match fs::remove_file(path) {
                Ok(()) => sync_directory(&self.directory),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(map_io(error)),
            },
        }
    }

    fn read(
        &self,
        name: &str,
        maximum: usize,
    ) -> Result<Option<Vec<u8>>, DesktopProductStoreError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| DesktopProductStoreError::Unavailable)?;
        let path = self.directory.join(name);
        match fs::read(path) {
            Ok(value) if value.len() <= maximum => Ok(Some(value)),
            Ok(_) => Err(DesktopProductStoreError::InvalidValue),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(map_io(error)),
        }
    }

    fn write(
        &self,
        name: &str,
        value: &[u8],
        maximum: usize,
    ) -> Result<(), DesktopProductStoreError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| DesktopProductStoreError::Unavailable)?;
        fs::create_dir_all(&self.directory).map_err(map_io)?;
        write_atomic(&self.directory, &self.directory.join(name), value, maximum)
    }
}

fn write_atomic(
    directory: &Path,
    destination: &Path,
    value: &[u8],
    maximum: usize,
) -> Result<(), DesktopProductStoreError> {
    if value.is_empty() || value.len() > maximum {
        return Err(DesktopProductStoreError::InvalidValue);
    }
    let temporary = destination.with_extension("json.pending");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(map_io)?;
    file.write_all(value).map_err(map_io)?;
    file.sync_all().map_err(map_io)?;
    fs::rename(&temporary, destination).map_err(map_io)?;
    sync_directory(directory)
}

fn sync_directory(directory: &Path) -> Result<(), DesktopProductStoreError> {
    #[cfg(unix)]
    {
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(map_io)
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        Ok(())
    }
}

fn map_io(_: io::Error) -> DesktopProductStoreError {
    DesktopProductStoreError::Unavailable
}
