package com.garive.eng.kt.postgres

import com.garive.eng.kt.ledger.CommitDisposition
import com.garive.eng.kt.ledger.CommitResult
import com.garive.eng.kt.ledger.DurableFact
import com.garive.eng.kt.ledger.FactDraft
import com.garive.eng.kt.ledger.FactKind
import com.garive.eng.kt.ledger.LedgerError
import com.garive.eng.kt.ledger.LedgerResult
import com.garive.eng.kt.ledger.ModelRequestId
import com.garive.eng.kt.ledger.SessionId
import com.garive.eng.kt.ledger.TurnId
import com.garive.eng.kt.ledger.TurnSnapshot
import com.garive.eng.kt.ledger.ToolInvocationId
import java.math.BigDecimal
import java.sql.Connection
import java.sql.SQLException
import java.time.OffsetDateTime

/** PostgreSQL durable Ledger experiment using real serializable transactions. */
public class PostgresLedger private constructor(private val config: PostgresConfig) {
    public companion object {
        /** Opens the adapter and applies/refuses schema migrations before use. */
        public fun open(config: PostgresConfig): PostgresLedger {
            try {
                config.connect().use(PostgresMigrations::migrate)
            } catch (error: PostgresLedgerError) {
                throw error
            } catch (error: SQLException) {
                throw PostgresLedgerError.Storage(error)
            }
            return PostgresLedger(config)
        }
    }

    /** Atomically validates and appends a batch at an expected Session version. */
    public fun commit(
        sessionId: SessionId,
        expectedSessionVersion: ULong,
        drafts: List<FactDraft>,
    ): CommitResult = transaction(Connection.TRANSACTION_SERIALIZABLE) { connection ->
        lockIdentities(connection, sessionId, drafts)
        connection.prepareStatement(
            "INSERT INTO ledger_sessions(session_id, version, max_position) VALUES (?, 0, 0) " +
                "ON CONFLICT (session_id) DO NOTHING",
        ).use {
            it.setString(1, sessionId.value)
            it.executeUpdate()
        }
        val storedVersion = connection.prepareStatement(
            "SELECT version FROM ledger_sessions WHERE session_id = ? FOR UPDATE",
        ).use {
            it.setString(1, sessionId.value)
            it.executeQuery().use { rows ->
                if (!rows.next()) corrupt("missing locked session")
                rows.getBigDecimal(1).toBigIntegerExact().toString().toULong()
            }
        }
        val state = PostgresStorage.loadState(connection)
        val result = when (val committed = state.commit(sessionId, expectedSessionVersion, drafts)) {
            is LedgerResult.Failure -> throw PostgresLedgerError.Domain(committed.error)
            is LedgerResult.Success -> committed.value
        }
        if (result.disposition == CommitDisposition.REPLAYED) return@transaction result
        if (storedVersion != expectedSessionVersion) {
            throw PostgresLedgerError.Domain(LedgerError.ConcurrentModification)
        }
        insertFacts(connection, sessionId, result, drafts)
        connection.prepareStatement(
            "UPDATE ledger_sessions SET version = ?, max_position = ? WHERE session_id = ? AND version = ?",
        ).use {
            it.setBigDecimal(1, result.sessionVersion.decimal())
            it.setBigDecimal(2, requireNotNull(result.positions.lastOrNull()).decimal())
            it.setString(3, sessionId.value)
            it.setBigDecimal(4, expectedSessionVersion.decimal())
            if (it.executeUpdate() != 1) {
                throw PostgresLedgerError.Domain(LedgerError.ConcurrentModification)
            }
        }
        result
    }

    /** Reads a verified fixed-prefix fact range in durable position order. */
    public fun readFacts(
        sessionId: SessionId,
        afterPosition: ULong,
        throughPosition: ULong,
        kinds: Set<FactKind>? = null,
    ): List<DurableFact> = readState { state ->
        when (val result = state.readFacts(sessionId, afterPosition, throughPosition, kinds)) {
            is LedgerResult.Failure -> throw PostgresLedgerError.Domain(result.error)
            is LedgerResult.Success -> result.value
        }
    }

    /** Lists model requests still Started without a recovery-terminal fact. */
    public fun listUncertainModelRequests(sessionId: SessionId): List<ModelRequestId> = readState { state ->
        when (val result = state.listUncertainModelRequests(sessionId)) {
            is LedgerResult.Failure -> throw PostgresLedgerError.Domain(result.error)
            is LedgerResult.Success -> result.value
        }
    }

    /** Lists effects still Started without a receipt or terminal fact. */
    public fun listUncertainToolInvocations(sessionId: SessionId): List<ToolInvocationId> = readState { state ->
        when (val result = state.listUncertainToolInvocations(sessionId)) {
            is LedgerResult.Failure -> throw PostgresLedgerError.Domain(result.error)
            is LedgerResult.Success -> result.value
        }
    }

    /** Returns the current durable optimistic-concurrency Session version. */
    public fun sessionVersion(sessionId: SessionId): ULong? = readState { it.sessionVersion(sessionId) }

    /** Loads one verified Turn and its containing Session watermark repeatably. */
    public fun loadTurn(turnId: TurnId): TurnSnapshot = readState { state ->
        when (val result = state.loadTurn(turnId)) {
            is LedgerResult.Failure -> throw PostgresLedgerError.Domain(result.error)
            is LedgerResult.Success -> result.value
        }
    }

    private fun insertFacts(
        connection: Connection,
        sessionId: SessionId,
        result: CommitResult,
        drafts: List<FactDraft>,
    ) {
        val sql = """
            INSERT INTO ledger_facts(
                fact_id, session_id, position, commit_version, turn_id, execution_id,
                model_request_id, tool_invocation_id, kind, schema_version, payload,
                payload_sha256, recorded_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?::jsonb, ?, ?)
        """.trimIndent()
        connection.prepareStatement(sql).use { statement ->
            drafts.zip(result.positions).forEach { (draft, position) ->
                val recordedAt = try {
                    OffsetDateTime.parse(draft.recordedAt)
                } catch (_: RuntimeException) {
                    throw PostgresLedgerError.Domain(LedgerError.InvalidFact)
                }
                statement.setString(1, draft.factId.value)
                statement.setString(2, sessionId.value)
                statement.setBigDecimal(3, position.decimal())
                statement.setBigDecimal(4, result.sessionVersion.decimal())
                statement.setString(5, draft.turnId?.value)
                statement.setString(6, draft.executionId?.value)
                statement.setString(7, draft.modelRequestId?.value)
                statement.setString(8, draft.toolInvocationId?.value)
                statement.setString(9, draft.kind.value)
                statement.setLong(10, draft.schemaVersion.toLong())
                statement.setString(11, draft.payload.json)
                statement.setString(12, draft.payload.sha256)
                statement.setObject(13, recordedAt)
                statement.addBatch()
            }
            statement.executeBatch()
        }
    }

    private fun lockIdentities(connection: Connection, sessionId: SessionId, drafts: List<FactDraft>) {
        val identities = drafts.map { it.factId.value }.distinct().sorted()
        connection.prepareStatement(
            "SELECT pg_advisory_xact_lock(hashtextextended(?, 7190418305))",
        ).use { statement ->
            (listOf("session:${sessionId.value}") + identities.map { "fact:$it" }).forEach {
                statement.setString(1, it)
                statement.executeQuery().close()
            }
        }
    }

    private fun <T> readState(block: (com.garive.eng.kt.ledger.LedgerState) -> T): T =
        transaction(Connection.TRANSACTION_REPEATABLE_READ) { block(PostgresStorage.loadState(it)) }

    private fun <T> transaction(isolation: Int, block: (Connection) -> T): T {
        try {
            config.connect().use { connection ->
                connection.autoCommit = false
                connection.transactionIsolation = isolation
                try {
                    configureTimeouts(connection)
                    val result = block(connection)
                    connection.commit()
                    return result
                } catch (error: Throwable) {
                    connection.rollback()
                    throw error
                }
            }
        } catch (error: PostgresLedgerError) {
            throw error
        } catch (error: SQLException) {
            if (error.sqlState == "40001") {
                throw PostgresLedgerError.Domain(LedgerError.ConcurrentModification)
            }
            throw PostgresLedgerError.Storage(error)
        }
    }

    private fun configureTimeouts(connection: Connection) {
        connection.prepareStatement("SELECT set_config('statement_timeout', ?, true)").use {
            it.setString(1, "${config.statementTimeoutMs}ms")
            it.executeQuery().close()
        }
        connection.prepareStatement("SELECT set_config('lock_timeout', ?, true)").use {
            it.setString(1, "${config.lockTimeoutMs}ms")
            it.executeQuery().close()
        }
    }
}

private fun ULong.decimal() = BigDecimal(toString())
private fun corrupt(detail: String): Nothing = throw PostgresLedgerError.Corrupt(detail)
