use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
};

use serde::Serialize;

/// Stable failure while reserving or committing an experiment evidence file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceFileError {
    /// The destination could not be created exclusively.
    CreateFailed,
    /// The evidence value could not be encoded.
    EncodeFailed,
    /// The reserved file could not be written and synchronized.
    WriteFailed,
}

impl EvidenceFileError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::CreateFailed => "evidence_create_failed",
            Self::EncodeFailed => "evidence_encode_failed",
            Self::WriteFailed => "evidence_write_failed",
        }
    }
}

/// Exclusively created destination removed if dropped before a successful commit.
pub struct EvidenceFileReservation {
    path: PathBuf,
    file: Option<File>,
}

/// Reserves a non-overwriting evidence destination before secrets or effects.
pub fn reserve_evidence_file(path: PathBuf) -> Result<EvidenceFileReservation, EvidenceFileError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| EvidenceFileError::CreateFailed)?;
    Ok(EvidenceFileReservation {
        path,
        file: Some(file),
    })
}

impl EvidenceFileReservation {
    /// Writes pretty JSON plus a newline, synchronizes, and commits the reservation.
    pub fn commit_json(&mut self, evidence: &impl Serialize) -> Result<(), EvidenceFileError> {
        let bytes =
            serde_json::to_vec_pretty(evidence).map_err(|_| EvidenceFileError::EncodeFailed)?;
        let file = self.file.as_mut().ok_or(EvidenceFileError::WriteFailed)?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|_| EvidenceFileError::WriteFailed)?;
        self.file.take();
        Ok(())
    }
}

impl Drop for EvidenceFileReservation {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = fs::remove_file(&self.path);
        }
    }
}
