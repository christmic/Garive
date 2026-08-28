package com.garive.runtime.server.postgres

import java.sql.Connection

internal object PostgresMigrations {
    private const val latestVersion = 1

    fun migrate(connection: Connection) {
        connection.autoCommit = false
        try {
            connection.createStatement().use { statement ->
                statement.execute("SELECT pg_advisory_xact_lock(7190418305672093801)")
                statement.execute(
                    """
                    CREATE TABLE IF NOT EXISTS ledger_schema_migrations (
                        version integer PRIMARY KEY,
                        applied_at timestamptz NOT NULL DEFAULT now()
                    )
                    """.trimIndent(),
                )
            }
            val version = connection.createStatement().use { statement ->
                statement.executeQuery("SELECT COALESCE(MAX(version), 0) FROM ledger_schema_migrations").use {
                    require(it.next())
                    it.getInt(1)
                }
            }
            if (version > latestVersion) throw PostgresLedgerError.UnsupportedSchema(version)
            if (version == 0) {
                val sql = requireNotNull(
                    javaClass.getResource("/db/migration/V1__ledger.sql"),
                    { "missing PostgreSQL ledger migration" },
                ).readText()
                connection.createStatement().use { it.execute(sql) }
                connection.prepareStatement(
                    "INSERT INTO ledger_schema_migrations(version) VALUES (?)",
                ).use {
                    it.setInt(1, latestVersion)
                    it.executeUpdate()
                }
            }
            connection.commit()
        } catch (error: Throwable) {
            connection.rollback()
            throw error
        } finally {
            connection.autoCommit = true
        }
    }
}
