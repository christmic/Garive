use garive_ledger::{
    validate_runtime_fact, CanonicalPayload, FactDraft, FactId, FactKind, LedgerError,
    RuntimeFactDisposition, TurnId,
};
use serde_json::json;

fn context_fact(payload: serde_json::Value) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("workspace-context-fact").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("workspace.context_selected").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-08-30T00:00:00Z".into(),
    }
}

fn valid_payload() -> serde_json::Value {
    json!({
        "command_id":"start-with-context",
        "workspace_id":"workspace-opaque",
        "grant_revision":1,
        "entries":[{
            "entry_id":"entry-opaque",
            "display_name":"brief.md",
            "kind":"text",
            "content":{
                "digest":"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
                "inline_utf8":"hello"
            }
        }]
    })
}

#[test]
fn selected_workspace_context_is_a_strict_session_scoped_fact() {
    assert_eq!(
        validate_runtime_fact(&context_fact(valid_payload())),
        Ok(RuntimeFactDisposition::AppliedV1)
    );
    let mut turn_scoped = context_fact(valid_payload());
    turn_scoped.turn_id = Some(TurnId::try_from("turn-forbidden").unwrap());
    assert_eq!(
        validate_runtime_fact(&turn_scoped),
        Err(LedgerError::InvalidFact)
    );
}

#[test]
fn selected_workspace_context_rejects_tampering_and_unbounded_entries() {
    let mut digest = valid_payload();
    digest["entries"][0]["content"]["digest"] = json!("0".repeat(64));
    assert_eq!(
        validate_runtime_fact(&context_fact(digest)),
        Err(LedgerError::InvalidFact)
    );
    let mut extra = valid_payload();
    extra["entries"][0]["path"] = json!("/private/brief.md");
    assert_eq!(
        validate_runtime_fact(&context_fact(extra)),
        Err(LedgerError::InvalidFact)
    );
    let mut many = valid_payload();
    many["entries"] = json!(vec![valid_payload()["entries"][0].clone(); 9]);
    assert_eq!(
        validate_runtime_fact(&context_fact(many)),
        Err(LedgerError::InvalidFact)
    );
}
