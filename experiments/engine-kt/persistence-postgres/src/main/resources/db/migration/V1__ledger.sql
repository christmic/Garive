CREATE TABLE ledger_sessions (
    session_id text PRIMARY KEY,
    version numeric(20, 0) NOT NULL CHECK (version BETWEEN 0 AND 18446744073709551615),
    max_position numeric(20, 0) NOT NULL CHECK (max_position BETWEEN 0 AND 18446744073709551615)
);

CREATE TABLE ledger_facts (
    fact_id text PRIMARY KEY,
    session_id text NOT NULL REFERENCES ledger_sessions(session_id),
    position numeric(20, 0) NOT NULL CHECK (position BETWEEN 1 AND 18446744073709551615),
    commit_version numeric(20, 0) NOT NULL CHECK (commit_version BETWEEN 1 AND 18446744073709551615),
    turn_id text,
    execution_id text,
    model_request_id text,
    tool_invocation_id text,
    kind text NOT NULL,
    schema_version bigint NOT NULL CHECK (schema_version BETWEEN 1 AND 4294967295),
    payload jsonb NOT NULL,
    payload_sha256 char(64) NOT NULL CHECK (payload_sha256 ~ '^[0-9a-f]{64}$'),
    recorded_at timestamptz NOT NULL,
    UNIQUE (session_id, position)
);

CREATE UNIQUE INDEX one_model_prepared
    ON ledger_facts(model_request_id)
    WHERE kind = 'model.prepared';
CREATE UNIQUE INDEX one_effect_prepared
    ON ledger_facts(tool_invocation_id)
    WHERE kind = 'effect.prepared';
CREATE INDEX facts_by_session_version_position
    ON ledger_facts(session_id, commit_version, position);
CREATE INDEX facts_by_model_request
    ON ledger_facts(model_request_id, position)
    WHERE model_request_id IS NOT NULL;
CREATE INDEX facts_by_tool_invocation
    ON ledger_facts(tool_invocation_id, position)
    WHERE tool_invocation_id IS NOT NULL;
