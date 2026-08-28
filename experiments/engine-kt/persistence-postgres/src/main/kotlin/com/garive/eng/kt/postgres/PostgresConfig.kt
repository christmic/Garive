package com.garive.eng.kt.postgres

import java.sql.Connection
import java.sql.DriverManager

data class PostgresConfig(
    val jdbcUrl: String,
    val username: String,
    val password: String,
    val statementTimeoutMs: UInt = 5_000u,
    val lockTimeoutMs: UInt = 2_000u,
) {
    init {
        require(jdbcUrl.startsWith("jdbc:postgresql:"))
        require(username.isNotEmpty())
        require(statementTimeoutMs > 0u)
        require(lockTimeoutMs > 0u)
    }

    internal fun connect(): Connection = DriverManager.getConnection(jdbcUrl, username, password)
}

sealed class PostgresLedgerError(val code: String, cause: Throwable? = null) : Exception(code, cause) {
    class Domain(val error: com.garive.eng.kt.ledger.LedgerError) :
        PostgresLedgerError(error.code)
    class Storage(cause: Throwable) : PostgresLedgerError("postgres-storage", cause)
    class Corrupt(val detail: String, cause: Throwable? = null) :
        PostgresLedgerError("ledger-corruption:$detail", cause)
    class UnsupportedSchema(val version: Int) : PostgresLedgerError("unsupported-schema:$version")
}
