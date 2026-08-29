package com.garive.eng.kt.postgres

import java.sql.Connection
import java.sql.DriverManager

/**
 * PostgreSQL connection policy for the experimental adapter.
 *
 * Password is intentionally excluded from data-class printing/copy/equality.
 */
public class PostgresConfig(
    public val jdbcUrl: String,
    public val username: String,
    password: String,
    public val statementTimeoutMs: UInt = 5_000u,
    public val lockTimeoutMs: UInt = 2_000u,
) {
    internal val password: String = password

    init {
        require(jdbcUrl.startsWith("jdbc:postgresql:"))
        require(username.isNotEmpty())
        require(statementTimeoutMs > 0u)
        require(lockTimeoutMs > 0u)
    }

    internal fun connect(): Connection = DriverManager.getConnection(jdbcUrl, username, password)
}

/** Domain, integrity, migration, or storage failure from [PostgresLedger]. */
public sealed class PostgresLedgerError protected constructor(
    public val code: String,
    cause: Throwable? = null,
) : Exception(code, cause) {
    public class Domain(public val error: com.garive.eng.kt.ledger.LedgerError) :
        PostgresLedgerError(error.code)
    public class Storage(cause: Throwable) : PostgresLedgerError("postgres-storage", cause)
    public class Corrupt(public val detail: String, cause: Throwable? = null) :
        PostgresLedgerError("ledger-corruption:$detail", cause)
    public class UnsupportedSchema(public val version: Int) : PostgresLedgerError("unsupported-schema:$version")
}
