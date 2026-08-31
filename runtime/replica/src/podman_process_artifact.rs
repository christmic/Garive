//! Runtime-private environment artifact lifecycle for Podman create.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub(crate) struct EnvironmentArtifact {
    path: PathBuf,
}

impl EnvironmentArtifact {
    pub(crate) fn create(
        root: &Path,
        name: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, ()> {
        let path = root.join(format!(".{name}.env"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&path).map_err(|_| ())?;
        write_environment(&mut file, environment)?;
        file.sync_all().map_err(|_| ())?;
        sync_directory(root)?;
        Ok(Self { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn remove(self) -> Result<(), ()> {
        fs::remove_file(&self.path).map_err(|_| ())?;
        sync_directory(self.path.parent().ok_or(())?)
    }

    pub(crate) fn remove_if_present(root: &Path, name: &str) -> Result<(), ()> {
        let path = root.join(format!(".{name}.env"));
        match fs::remove_file(path) {
            Ok(()) => sync_directory(root),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(()),
        }
    }
}

fn write_environment(file: &mut File, environment: &BTreeMap<String, String>) -> Result<(), ()> {
    for (key, value) in environment {
        file.write_all(key.as_bytes()).map_err(|_| ())?;
        file.write_all(b"=").map_err(|_| ())?;
        file.write_all(value.as_bytes()).map_err(|_| ())?;
        file.write_all(b"\n").map_err(|_| ())?;
    }
    file.flush().map_err(|_| ())
}

fn sync_directory(path: &Path) -> Result<(), ()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| ())
}
