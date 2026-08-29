package com.garive.eng.kt.postgres

import com.garive.eng.kt.ledger.CanonicalPayload
import com.garive.eng.kt.ledger.CanonicalPayloadResult
import com.garive.eng.kt.ledger.CommitDisposition
import com.garive.eng.kt.ledger.ExecutionId
import com.garive.eng.kt.ledger.FactDraft
import com.garive.eng.kt.ledger.FactId
import com.garive.eng.kt.ledger.FactKind
import com.garive.eng.kt.ledger.LedgerResult
import com.garive.eng.kt.ledger.LedgerState
import com.garive.eng.kt.ledger.ModelRequestId
import com.garive.eng.kt.ledger.SessionId
import com.garive.eng.kt.ledger.ToolInvocationId
import com.garive.eng.kt.ledger.TurnId
import java.math.BigDecimal
import java.sql.Connection
import java.sql.ResultSet
import java.time.OffsetDateTime
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

internal object PostgresStorage {
    fun loadState(connection: Connection): LedgerState {
        val state = LedgerState()
        var currentSession: SessionId? = null
        var currentVersion = 0uL
        var expectedPosition = 1uL
        var group = mutableListOf<FactDraft>()
        connection.createStatement().use { statement ->
            statement.executeQuery(
                """
                SELECT session_id, position, commit_version, fact_id, turn_id, execution_id,
                       model_request_id, tool_invocation_id, kind, schema_version,
                       payload::text AS payload_json, payload_sha256, recorded_at
                FROM ledger_facts
                ORDER BY session_id, commit_version, position
                """.trimIndent(),
            ).use { rows ->
                while (rows.next()) {
                    val session = SessionId.of(rows.getString("session_id"))
                    val version = rows.unsigned("commit_version")
                    val position = rows.unsigned("position")
                    val sessionChanged = currentSession != null && currentSession != session
                    if (currentSession != null && (sessionChanged || currentVersion != version)) {
                        applyGroup(state, requireNotNull(currentSession), currentVersion, group)
                        group = mutableListOf()
                        if (sessionChanged) expectedPosition = 1uL
                    }
                    if (currentSession == null || sessionChanged) currentSession = session
                    currentVersion = version
                    if (position != expectedPosition) corrupt("non-contiguous position")
                    expectedPosition = position.incrementOrCorrupt("position overflow")
                    group += rows.draft()
                }
            }
        }
        currentSession?.let { applyGroup(state, it, currentVersion, group) }
        verifySessionRows(connection, state)
        return state
    }

    private fun applyGroup(
        state: LedgerState,
        sessionId: SessionId,
        version: ULong,
        drafts: List<FactDraft>,
    ) {
        val expected = if (version == 0uL) corrupt("zero commit version") else version - 1u
        when (val result = state.commit(sessionId, expected, drafts)) {
            is LedgerResult.Failure -> corrupt("invalid fact stream:${result.error.code}")
            is LedgerResult.Success -> if (
                result.value.disposition != CommitDisposition.COMMITTED ||
                result.value.sessionVersion != version
            ) {
                corrupt("invalid commit version")
            }
        }
    }

    private fun ResultSet.draft(): FactDraft {
        val value = try {
            Json.parseToJsonElement(getString("payload_json"))
        } catch (_: SerializationException) {
            corrupt("invalid payload json")
        } catch (_: IllegalArgumentException) {
            corrupt("invalid payload json")
        }
        val payload = when (val result = CanonicalPayload.fromValue(value)) {
            is CanonicalPayloadResult.Failure -> corrupt("invalid payload:${result.error}")
            is CanonicalPayloadResult.Success -> result.payload
        }
        if (payload.sha256 != getString("payload_sha256").trimEnd()) {
            corrupt("invalid payload:DIGEST_MISMATCH")
        }
        val schema = getLong("schema_version")
        if (schema !in 1..UInt.MAX_VALUE.toLong()) corrupt("invalid schema version")
        return FactDraft(
            FactId.of(getString("fact_id")),
            optional("turn_id", TurnId::of),
            optional("execution_id", ExecutionId::of),
            optional("model_request_id", ModelRequestId::of),
            optional("tool_invocation_id", ToolInvocationId::of),
            FactKind.of(getString("kind")),
            schema.toUInt(),
            payload,
            getObject("recorded_at", OffsetDateTime::class.java).toString(),
        )
    }

    private fun verifySessionRows(connection: Connection, state: LedgerState) {
        connection.createStatement().use { statement ->
            statement.executeQuery(
                "SELECT session_id, version, max_position FROM ledger_sessions ORDER BY session_id",
            ).use { rows ->
                while (rows.next()) {
                    val session = SessionId.of(rows.getString("session_id"))
                    val version = rows.unsigned("version")
                    val maxPosition = rows.unsigned("max_position")
                    if (version == 0uL && maxPosition == 0uL && state.sessionVersion(session) == null) continue
                    if (state.sessionVersion(session) != version || state.factCount(session).toULong() != maxPosition) {
                        corrupt("session projection mismatch")
                    }
                }
            }
        }
    }

    private fun ResultSet.unsigned(column: String): ULong = try {
        getObject(column, BigDecimal::class.java).toBigIntegerExact().toString().toULong()
    } catch (error: RuntimeException) {
        throw PostgresLedgerError.Corrupt("invalid $column", error)
    }

    private fun <T> ResultSet.optional(column: String, transform: (String) -> T): T? =
        getString(column)?.let(transform)

    private fun ULong.incrementOrCorrupt(detail: String) =
        if (this == ULong.MAX_VALUE) corrupt(detail) else this + 1u

    private fun corrupt(detail: String): Nothing = throw PostgresLedgerError.Corrupt(detail)
}
