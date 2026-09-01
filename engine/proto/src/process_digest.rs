use sha2::{Digest, Sha256};

use crate::com::garive::process::v1::{
    process_exit_v1, ProcessIdentityV1, ProcessTerminalReceiptV1, ProcessWorkloadV1,
    ProcessWorkspaceModeV1,
};

const MAX_ARGUMENTS: usize = 256;
const MAX_ARGUMENT_BYTES: usize = 16_384;
const MAX_ARGUMENTS_TOTAL_BYTES: usize = 262_144;
const MAX_ENVIRONMENT_ENTRIES: usize = 128;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 16_384;
const MAX_ENVIRONMENT_TOTAL_BYTES: usize = 262_144;
const MAX_OUTPUT_BYTES: usize = 1_048_576;

/// Closed failures for canonical process protocol digests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessDigestError {
    /// Runtime-owned identity is malformed or carries a mismatched digest.
    InvalidIdentity,
    /// Workload shape, ordering, path, or bound is invalid.
    InvalidWorkload,
    /// Terminal evidence is incomplete, unbounded, or digest-mismatched.
    InvalidReceipt,
}

/// Validates and computes the canonical V0-B workload digest.
pub fn process_workload_digest(
    identity: &ProcessIdentityV1,
    workload: &ProcessWorkloadV1,
) -> Result<[u8; 32], ProcessDigestError> {
    if !valid_identity(identity) {
        return Err(ProcessDigestError::InvalidIdentity);
    }
    let mode = validate_workload(workload)?;
    let mut input = DigestInput::new("garive.macos-process-workload.v1");
    for value in [
        identity.protocol_revision.as_bytes(),
        identity.invocation_id.as_bytes(),
        identity.dispatch_attempt_id.as_bytes(),
        identity.executor_revision.as_bytes(),
        &identity.prepared_digest,
        &identity.vm_configuration_digest,
        workload.lane.as_bytes(),
        workload.executable.as_bytes(),
    ] {
        input.field(value);
    }
    input.number(workload.argv.len() as u64);
    for value in &workload.argv {
        input.field(value.as_bytes());
    }
    input.field(workload.working_directory.as_bytes());
    input.byte(mode);
    input.number(workload.environment.len() as u64);
    for entry in &workload.environment {
        input.field(entry.key.as_bytes());
        input.field(entry.value.as_bytes());
    }
    for value in [
        workload.max_output_bytes,
        workload.timeout_milliseconds,
        u64::from(workload.max_processes),
        u64::from(workload.max_open_files),
    ] {
        input.number(value);
    }
    let digest = input.finish();
    if !identity.workload_digest.is_empty() && identity.workload_digest != digest {
        return Err(ProcessDigestError::InvalidIdentity);
    }
    Ok(digest)
}

/// Validates and computes the canonical V0-B terminal receipt digest.
pub fn process_receipt_digest(
    receipt: &ProcessTerminalReceiptV1,
) -> Result<[u8; 32], ProcessDigestError> {
    let identity = receipt
        .identity
        .as_ref()
        .filter(|value| valid_identity(value) && value.workload_digest.len() == 32)
        .ok_or(ProcessDigestError::InvalidReceipt)?;
    if !receipt.process_tree_terminated
        || receipt
            .stdout
            .len()
            .checked_add(receipt.stderr.len())
            .is_none_or(|total| total > MAX_OUTPUT_BYTES)
    {
        return Err(ProcessDigestError::InvalidReceipt);
    }
    let classification = receipt
        .exit
        .as_ref()
        .and_then(|exit| exit.classification.as_ref())
        .ok_or(ProcessDigestError::InvalidReceipt)?;
    let mut input = DigestInput::new("garive.macos-process-receipt.v1");
    input.field(&identity.workload_digest);
    match classification {
        process_exit_v1::Classification::Code(value) => {
            input.byte(0);
            input.signed(*value);
        }
        process_exit_v1::Classification::Signal(value) if *value > 0 => {
            input.byte(1);
            input.signed(*value);
        }
        process_exit_v1::Classification::TimedOut(true) => input.byte(2),
        _ => return Err(ProcessDigestError::InvalidReceipt),
    }
    input.field(&receipt.stdout);
    input.field(&receipt.stderr);
    input.byte(u8::from(receipt.truncated));
    input.byte(1);
    let digest = input.finish();
    if !receipt.receipt_digest.is_empty() && receipt.receipt_digest != digest {
        return Err(ProcessDigestError::InvalidReceipt);
    }
    Ok(digest)
}

fn valid_identity(value: &ProcessIdentityV1) -> bool {
    valid_protocol_revision(&value.protocol_revision)
        && [
            value.invocation_id.as_str(),
            value.dispatch_attempt_id.as_str(),
            value.executor_revision.as_str(),
        ]
        .into_iter()
        .all(valid_identity_text)
        && value.prepared_digest.len() == 32
        && value.vm_configuration_digest.len() == 32
        && matches!(value.workload_digest.len(), 0 | 32)
}

fn validate_workload(value: &ProcessWorkloadV1) -> Result<u8, ProcessDigestError> {
    let mode = match ProcessWorkspaceModeV1::try_from(value.workspace_mode) {
        Ok(ProcessWorkspaceModeV1::ProcessWorkspaceModeReadOnly) => 1,
        Ok(ProcessWorkspaceModeV1::ProcessWorkspaceModeReadWrite) => 2,
        _ => return Err(ProcessDigestError::InvalidWorkload),
    };
    let arguments_bytes = value.argv.iter().try_fold(0_usize, |total, argument| {
        let length = argument.len();
        (length > 0 && length <= MAX_ARGUMENT_BYTES && !argument.contains('\0'))
            .then(|| total.checked_add(length))
            .flatten()
    });
    let mut prior_key: Option<&str> = None;
    let environment_bytes = value.environment.iter().try_fold(0_usize, |total, entry| {
        let ordered = prior_key.is_none_or(|prior| prior.as_bytes() < entry.key.as_bytes());
        prior_key = Some(&entry.key);
        (ordered
            && valid_environment_key(&entry.key)
            && entry.value.len() <= MAX_ENVIRONMENT_VALUE_BYTES
            && !entry.value.contains(['\0', '\r', '\n']))
        .then(|| total.checked_add(entry.key.len() + entry.value.len()))
        .flatten()
    });
    if !valid_identity_text_with_bound(&value.lane, 128)
        || !valid_absolute_guest_path(&value.executable)
        || !valid_relative_workspace_path(&value.working_directory)
        || value.argv.is_empty()
        || value.argv.len() > MAX_ARGUMENTS
        || arguments_bytes.is_none_or(|total| total > MAX_ARGUMENTS_TOTAL_BYTES)
        || value.environment.len() > MAX_ENVIRONMENT_ENTRIES
        || environment_bytes.is_none_or(|total| total > MAX_ENVIRONMENT_TOTAL_BYTES)
        || value.max_output_bytes == 0
        || value.max_output_bytes > MAX_OUTPUT_BYTES as u64
        || !(1..=300_000).contains(&value.timeout_milliseconds)
        || value.max_processes == 0
        || value.max_open_files == 0
    {
        return Err(ProcessDigestError::InvalidWorkload);
    }
    Ok(mode)
}

fn valid_protocol_revision(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=128).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn valid_identity_text(value: &str) -> bool {
    valid_identity_text_with_bound(value, 256)
}

fn valid_identity_text_with_bound(value: &str, maximum: usize) -> bool {
    (1..=maximum).contains(&value.len())
        && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
        && !value.starts_with(' ')
        && !value.ends_with(' ')
}

fn valid_absolute_guest_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 4_096
        && value.len() > 1
        && value[1..]
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | "..") && !part.contains('\0'))
}

fn valid_relative_workspace_path(value: &str) -> bool {
    (value == "."
        || (!value.starts_with('/')
            && value
                .split('/')
                .all(|part| !part.is_empty() && !matches!(part, "." | ".."))))
        && (1..=4_096).contains(&value.len())
        && !value.contains('\0')
}

fn valid_environment_key(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && value.len() <= 128
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

struct DigestInput(Vec<u8>);

impl DigestInput {
    fn new(label: &str) -> Self {
        Self(label.as_bytes().to_vec())
    }

    fn field(&mut self, value: &[u8]) {
        self.number(value.len() as u64);
        self.0.extend(value);
    }

    fn number(&mut self, value: u64) {
        self.0.extend(value.to_be_bytes());
    }

    fn signed(&mut self, value: i32) {
        self.0.extend(value.to_be_bytes());
    }

    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    fn finish(self) -> [u8; 32] {
        Sha256::digest(self.0).into()
    }
}
