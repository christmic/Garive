//! Private recovery journal and descriptor-confined mutation for T1 patches.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
};

use garive_ledger::CanonicalPayload;
use garive_tools::{apply_t1_patch, EffectReceipt};
use rustix::{
    fd::OwnedFd,
    fs::{fsync, linkat, openat, renameat, unlinkat, AtFlags, Dir, FileType, Mode, OFlags},
    io::{dup, Errno},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAX_FILE_BYTES: u64 = 1_048_576;

pub(crate) fn execute_patch(
    root: OwnedFd,
    recovery: OwnedFd,
    invocation: &str,
    prepared_digest: &str,
    patch: &str,
    expected: &BTreeMap<String, String>,
    result_bound: u64,
) -> Result<Value, PatchFailure> {
    let key = short_digest(invocation.as_bytes());
    let journal_name = format!(".garive-patch-{key}.journal");
    let journal =
        match read_journal(&recovery, &journal_name).map_err(|_| PatchFailure::Uncertain)? {
            Some(value) => {
                validate_journal(&value, invocation, prepared_digest, &key, expected)?;
                value
            }
            None => prepare_journal(
                &root,
                &recovery,
                invocation,
                prepared_digest,
                &key,
                patch,
                expected,
            )?,
        };
    finish_journal(&root, &journal, patch).map_err(|_| PatchFailure::Uncertain)?;
    bounded(journal_result(&journal)?, result_bound)
}

pub(crate) fn acknowledge_patch(
    recovery: &OwnedFd,
    invocation: &str,
    receipt: &EffectReceipt,
) -> Result<(), PatchFailure> {
    let name = format!(
        ".garive-patch-{}.journal",
        short_digest(invocation.as_bytes())
    );
    let Some(journal) = read_journal(recovery, &name)? else {
        return Ok(());
    };
    if journal.invocation_id != invocation {
        return Err(PatchFailure::Uncertain);
    }
    let payload = CanonicalPayload::from_value(&journal_result(&journal)?)
        .map_err(|_| PatchFailure::Uncertain)?;
    if payload.sha256() != receipt.result_digest {
        return Err(PatchFailure::Uncertain);
    }
    unlinkat(recovery, name.as_str(), AtFlags::empty()).map_err(map_errno)?;
    fsync(recovery).map_err(map_errno)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PatchFailure {
    NotFound,
    AccessDenied,
    NonUtf8,
    BoundExceeded,
    ContentChanged,
    Conflict,
    Uncertain,
}

#[derive(Serialize, Deserialize)]
struct PatchJournal {
    invocation_id: String,
    prepared_digest: String,
    files: Vec<JournalFile>,
    receipt_digest: String,
}

#[derive(Serialize, Deserialize)]
struct JournalFile {
    path: String,
    before_digest: String,
    after_digest: String,
    temporary_name: String,
}

#[allow(clippy::too_many_arguments)]
fn prepare_journal(
    root: &OwnedFd,
    recovery: &OwnedFd,
    invocation: &str,
    prepared_digest: &str,
    key: &str,
    patch: &str,
    expected: &BTreeMap<String, String>,
) -> Result<PatchJournal, PatchFailure> {
    let mut files = Vec::new();
    let mut pending = Vec::new();
    for (index, (path, expected_digest)) in expected.iter().enumerate() {
        let (parent, name) = open_parent(root, path)?;
        let current = read_regular(&parent, &name)?;
        let before_digest = digest(&current);
        if &before_digest != expected_digest {
            return Err(PatchFailure::ContentChanged);
        }
        let current = String::from_utf8(current).map_err(|_| PatchFailure::NonUtf8)?;
        let next = apply_t1_patch(patch, path, &current).map_err(|_| PatchFailure::Conflict)?;
        if next.len() as u64 > MAX_FILE_BYTES {
            return Err(PatchFailure::BoundExceeded);
        }
        files.push(JournalFile {
            path: path.clone(),
            before_digest,
            after_digest: digest(next.as_bytes()),
            temporary_name: format!(".garive-patch-{key}-{index}.tmp"),
        });
        pending.push((parent, next.into_bytes()));
    }
    let receipt_digest = receipt_digest(&files)?;
    let journal = PatchJournal {
        invocation_id: invocation.into(),
        prepared_digest: prepared_digest.into(),
        files,
        receipt_digest,
    };
    write_journal(recovery, &format!(".garive-patch-{key}.journal"), &journal)
        .map_err(|_| PatchFailure::Uncertain)?;
    for ((parent, content), file) in pending.into_iter().zip(&journal.files) {
        write_new(&parent, &file.temporary_name, &content).map_err(|_| PatchFailure::Uncertain)?;
    }
    Ok(journal)
}

fn finish_journal(root: &OwnedFd, journal: &PatchJournal, patch: &str) -> Result<(), PatchFailure> {
    for file in &journal.files {
        let (parent, name) = open_parent(root, &file.path)?;
        let current = read_regular(&parent, &name)?;
        let current_digest = digest(&current);
        if current_digest == file.after_digest {
            continue;
        }
        if current_digest != file.before_digest {
            return Err(PatchFailure::Uncertain);
        }
        let current = String::from_utf8(current).map_err(|_| PatchFailure::Uncertain)?;
        let expected = apply_t1_patch(patch, &file.path, &current)
            .map_err(|_| PatchFailure::Uncertain)?
            .into_bytes();
        if digest(&expected) != file.after_digest {
            return Err(PatchFailure::Uncertain);
        }
        match read_regular(&parent, &file.temporary_name) {
            Ok(temporary) if digest(&temporary) == file.after_digest => {}
            Err(PatchFailure::NotFound) => write_new(&parent, &file.temporary_name, &expected)?,
            _ => return Err(PatchFailure::Uncertain),
        }
        renameat(
            &parent,
            file.temporary_name.as_str(),
            &parent,
            name.as_str(),
        )
        .map_err(map_errno)?;
        fsync(&parent).map_err(map_errno)?;
    }
    Ok(())
}

fn validate_journal(
    journal: &PatchJournal,
    invocation: &str,
    prepared_digest: &str,
    key: &str,
    expected: &BTreeMap<String, String>,
) -> Result<(), PatchFailure> {
    if journal.invocation_id != invocation
        || journal.prepared_digest != prepared_digest
        || journal.files.len() != expected.len()
        || !is_digest(&journal.receipt_digest)
    {
        return Err(PatchFailure::Uncertain);
    }
    for (index, (file, (path, before_digest))) in journal.files.iter().zip(expected).enumerate() {
        if file.path != *path
            || file.before_digest != *before_digest
            || !is_digest(&file.after_digest)
            || file.temporary_name != format!(".garive-patch-{key}-{index}.tmp")
        {
            return Err(PatchFailure::Uncertain);
        }
    }
    if receipt_digest(&journal.files)? != journal.receipt_digest {
        return Err(PatchFailure::Uncertain);
    }
    Ok(())
}

fn journal_result(journal: &PatchJournal) -> Result<Value, PatchFailure> {
    Ok(json!({
        "files":journal.files.iter().map(|file| json!({
            "path":file.path,"before_digest":file.before_digest,"after_digest":file.after_digest
        })).collect::<Vec<_>>(),
        "receipt_digest":journal.receipt_digest
    }))
}

fn receipt_digest(files: &[JournalFile]) -> Result<String, PatchFailure> {
    CanonicalPayload::from_value(&json!({"files":files.iter().map(|file| json!({
        "path":file.path,"before_digest":file.before_digest,"after_digest":file.after_digest
    })).collect::<Vec<_>>() }))
    .map_err(|_| PatchFailure::Uncertain)
    .map(|payload| payload.sha256().to_owned())
}

fn open_parent(root: &OwnedFd, path: &str) -> Result<(OwnedFd, String), PatchFailure> {
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts.pop().ok_or(PatchFailure::AccessDenied)?.to_owned();
    let mut current = dup(root).map_err(map_errno)?;
    for part in parts {
        if !has_exact(&current, part.as_bytes())? {
            return Err(PatchFailure::NotFound);
        }
        current = openat(
            &current,
            part,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(map_errno)?;
    }
    if !has_exact(&current, name.as_bytes())? {
        return Err(PatchFailure::NotFound);
    }
    Ok((current, name))
}

fn has_exact(directory: &OwnedFd, name: &[u8]) -> Result<bool, PatchFailure> {
    for entry in Dir::read_from(directory).map_err(map_errno)? {
        if entry.map_err(map_errno)?.file_name().to_bytes() == name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn read_regular(directory: &OwnedFd, name: &str) -> Result<Vec<u8>, PatchFailure> {
    if !has_exact(directory, name.as_bytes())? {
        return Err(PatchFailure::NotFound);
    }
    let fd = openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(map_errno)?;
    if FileType::from_raw_mode(rustix::fs::fstat(&fd).map_err(map_errno)?.st_mode)
        != FileType::RegularFile
    {
        return Err(PatchFailure::AccessDenied);
    }
    let mut bytes = Vec::new();
    File::from(fd)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PatchFailure::AccessDenied)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(PatchFailure::BoundExceeded);
    }
    Ok(bytes)
}

fn write_new(directory: &OwnedFd, name: &str, bytes: &[u8]) -> Result<(), PatchFailure> {
    let fd = openat(
        directory,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(map_errno)?;
    let mut file = File::from(fd);
    file.write_all(bytes)
        .map_err(|_| PatchFailure::AccessDenied)?;
    file.sync_all().map_err(|_| PatchFailure::AccessDenied)
}

fn read_journal(root: &OwnedFd, name: &str) -> Result<Option<PatchJournal>, PatchFailure> {
    if let Some(journal) = read_named_journal(root, name)? {
        return Ok(Some(journal));
    }
    let temporary = format!("{name}.tmp");
    let Some(journal) = read_named_journal(root, &temporary)? else {
        return Ok(None);
    };
    linkat(root, temporary.as_str(), root, name, AtFlags::empty()).map_err(map_errno)?;
    unlinkat(root, temporary.as_str(), AtFlags::empty()).map_err(map_errno)?;
    fsync(root).map_err(map_errno)?;
    Ok(Some(journal))
}

fn read_named_journal(root: &OwnedFd, name: &str) -> Result<Option<PatchJournal>, PatchFailure> {
    if !has_exact(root, name.as_bytes())? {
        return Ok(None);
    }
    let fd = match openat(
        root,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(map_errno(error)),
    };
    let mut bytes = Vec::new();
    File::from(fd)
        .take(2_097_153)
        .read_to_end(&mut bytes)
        .map_err(|_| PatchFailure::Uncertain)?;
    if bytes.len() > 2_097_152 {
        return Err(PatchFailure::Uncertain);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| PatchFailure::Uncertain)
}

fn write_journal(root: &OwnedFd, name: &str, journal: &PatchJournal) -> Result<(), PatchFailure> {
    let temporary = format!("{name}.tmp");
    write_new(
        root,
        &temporary,
        &serde_json::to_vec(journal).map_err(|_| PatchFailure::Uncertain)?,
    )?;
    linkat(root, temporary.as_str(), root, name, AtFlags::empty()).map_err(map_errno)?;
    unlinkat(root, temporary.as_str(), AtFlags::empty()).map_err(map_errno)?;
    fsync(root).map_err(map_errno)
}

fn bounded(value: Value, bound: u64) -> Result<Value, PatchFailure> {
    let payload = CanonicalPayload::from_value(&value).map_err(|_| PatchFailure::BoundExceeded)?;
    (payload.as_json().len() as u64 <= bound)
        .then_some(value)
        .ok_or(PatchFailure::BoundExceeded)
}

fn map_errno(error: Errno) -> PatchFailure {
    match error {
        Errno::NOENT => PatchFailure::NotFound,
        _ => PatchFailure::AccessDenied,
    }
}

fn short_digest(bytes: &[u8]) -> String {
    digest(bytes)[..24].to_owned()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
