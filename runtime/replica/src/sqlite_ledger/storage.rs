use std::mem;

use garive_ledger::{
    CanonicalPayload, CommitDisposition, FactDraft, FactId, FactKind, LedgerState, SessionId,
};
use rusqlite::{Connection, Transaction};

use super::SqliteLedgerError;

struct StoredFact {
    session_id: String,
    position: Vec<u8>,
    commit_version: Vec<u8>,
    fact_id: String,
    turn_id: Option<String>,
    execution_id: Option<String>,
    model_request_id: Option<String>,
    tool_invocation_id: Option<String>,
    kind: String,
    schema_version: i64,
    payload_json: String,
    payload_sha256: String,
    recorded_at: String,
}

pub(super) fn load_state(connection: &Connection) -> Result<LedgerState, SqliteLedgerError> {
    let transaction = connection.unchecked_transaction()?;
    let state = load_state_snapshot(&transaction)?;
    transaction.commit()?;
    Ok(state)
}

pub(super) fn load_state_in_transaction(
    transaction: &Transaction<'_>,
) -> Result<LedgerState, SqliteLedgerError> {
    load_state_snapshot(transaction)
}

fn load_state_snapshot(connection: &Connection) -> Result<LedgerState, SqliteLedgerError> {
    let mut statement = connection.prepare(
        "SELECT session_id, position, commit_version, fact_id, turn_id, execution_id, \
         model_request_id, tool_invocation_id, kind, schema_version, payload_json, \
         payload_sha256, recorded_at FROM ledger_facts \
         ORDER BY session_id, commit_version, position",
    )?;
    let stored = statement
        .query_map([], |row| {
            Ok(StoredFact {
                session_id: row.get(0)?,
                position: row.get(1)?,
                commit_version: row.get(2)?,
                fact_id: row.get(3)?,
                turn_id: row.get(4)?,
                execution_id: row.get(5)?,
                model_request_id: row.get(6)?,
                tool_invocation_id: row.get(7)?,
                kind: row.get(8)?,
                schema_version: row.get(9)?,
                payload_json: row.get(10)?,
                payload_sha256: row.get(11)?,
                recorded_at: row.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut state = LedgerState::default();
    let mut group_session: Option<SessionId> = None;
    let mut group_version = 0;
    let mut group = Vec::new();
    let mut expected_position = 1;
    for stored_fact in stored {
        let session = identity::<SessionId>(&stored_fact.session_id, "session_id")?;
        let version = decode_u64(&stored_fact.commit_version, "commit_version")?;
        let position = decode_u64(&stored_fact.position, "position")?;
        let session_changed = group_session
            .as_ref()
            .is_some_and(|current| current != &session);
        let boundary = group_session
            .as_ref()
            .is_some_and(|_| session_changed || group_version != version);
        if boundary {
            apply_group(
                &mut state,
                group_session.take().unwrap(),
                group_version,
                mem::take(&mut group),
            )?;
            if session_changed {
                expected_position = 1;
            }
        }
        if group_session.is_none() {
            group_session = Some(session);
            group_version = version;
        }
        if position != expected_position {
            return Err(SqliteLedgerError::InvalidStoredValue("position"));
        }
        expected_position = position
            .checked_add(1)
            .ok_or(SqliteLedgerError::InvalidStoredValue("position"))?;
        group.push(decode_draft(stored_fact)?);
    }
    if let Some(session) = group_session {
        apply_group(&mut state, session, group_version, group)?;
    }
    verify_session_rows(connection, &state)?;
    Ok(state)
}

fn apply_group(
    state: &mut LedgerState,
    session: SessionId,
    version: u64,
    drafts: Vec<FactDraft>,
) -> Result<(), SqliteLedgerError> {
    let expected = version
        .checked_sub(1)
        .ok_or(SqliteLedgerError::InvalidStoredValue("commit_version"))?;
    let result = state
        .commit(session, expected, drafts)
        .map_err(SqliteLedgerError::CorruptLedger)?;
    if result.disposition != CommitDisposition::Committed || result.session_version != version {
        return Err(SqliteLedgerError::InvalidStoredValue("commit_version"));
    }
    Ok(())
}

fn decode_draft(value: StoredFact) -> Result<FactDraft, SqliteLedgerError> {
    let schema_version = u32::try_from(value.schema_version)
        .map_err(|_| SqliteLedgerError::InvalidStoredValue("schema_version"))?;
    let payload = CanonicalPayload::from_canonical_parts(value.payload_json, value.payload_sha256)
        .map_err(|error| {
            SqliteLedgerError::CorruptLedger(garive_ledger::LedgerError::Corruption(error))
        })?;
    Ok(FactDraft {
        fact_id: identity::<FactId>(&value.fact_id, "fact_id")?,
        turn_id: optional_identity(value.turn_id, "turn_id")?,
        execution_id: optional_identity(value.execution_id, "execution_id")?,
        model_request_id: optional_identity(value.model_request_id, "model_request_id")?,
        tool_invocation_id: optional_identity(value.tool_invocation_id, "tool_invocation_id")?,
        kind: FactKind::new(value.kind).map_err(SqliteLedgerError::CorruptLedger)?,
        schema_version,
        payload,
        recorded_at: value.recorded_at,
    })
}

fn verify_session_rows(
    connection: &Connection,
    state: &LedgerState,
) -> Result<(), SqliteLedgerError> {
    let mut statement = connection.prepare(
        "SELECT session_id, version, max_position FROM ledger_sessions ORDER BY session_id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let session_text: String = row.get(0)?;
        let session = identity::<SessionId>(&session_text, "session_id")?;
        let version = decode_u64(&row.get::<_, Vec<u8>>(1)?, "version")?;
        let max_position = decode_u64(&row.get::<_, Vec<u8>>(2)?, "max_position")?;
        if state.session_version(&session) != Some(version)
            || state.fact_count(&session) as u64 != max_position
        {
            return Err(SqliteLedgerError::InvalidStoredValue("session projection"));
        }
    }
    Ok(())
}

fn identity<T>(value: &str, field: &'static str) -> Result<T, SqliteLedgerError>
where
    for<'a> T: TryFrom<&'a str>,
{
    T::try_from(value).map_err(|_| SqliteLedgerError::InvalidStoredValue(field))
}

fn optional_identity<T>(
    value: Option<String>,
    field: &'static str,
) -> Result<Option<T>, SqliteLedgerError>
where
    for<'a> T: TryFrom<&'a str>,
{
    value
        .as_deref()
        .map(|value| identity(value, field))
        .transpose()
}

pub(super) fn encode_u64(value: u64) -> [u8; 8] {
    value.to_be_bytes()
}

fn decode_u64(value: &[u8], field: &'static str) -> Result<u64, SqliteLedgerError> {
    let bytes: [u8; 8] = value
        .try_into()
        .map_err(|_| SqliteLedgerError::InvalidStoredValue(field))?;
    Ok(u64::from_be_bytes(bytes))
}
