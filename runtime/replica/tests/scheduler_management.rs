use std::cell::{Cell, RefCell};

use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_runtime::{
    cancel_schedule, create_schedule, reconstruct_schedule_state, update_schedule,
    ScheduleAuthorityOperation, ScheduleAuthorityPort, ScheduleCancelReason, SqliteLedger,
};
use garive_scheduler::{
    MisfirePolicy, ScheduleErrorCode, ScheduleIntent, ScheduleSubject, ScheduleTiming,
};
use serde_json::json;
use tempfile::tempdir;

struct Authority {
    allowed: Cell<bool>,
    operations: RefCell<Vec<ScheduleAuthorityOperation>>,
}
impl ScheduleAuthorityPort for Authority {
    fn authorize(
        &self,
        _: &SessionId,
        _: &ScheduleIntent,
        operation: ScheduleAuthorityOperation,
    ) -> Result<(), ScheduleErrorCode> {
        self.operations.borrow_mut().push(operation);
        if self.allowed.get() {
            Ok(())
        } else {
            Err(ScheduleErrorCode::AuthorityDenied)
        }
    }
}

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("management-session").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn intent(revision: &str, due: &str) -> ScheduleIntent {
    ScheduleIntent::new(
        "schedule-1",
        revision,
        ScheduleSubject::StartTurn,
        "aa".repeat(32),
        ScheduleTiming::At {
            due_at_utc: due.into(),
        },
        MisfirePolicy::FireOnce,
        500,
        "bb".repeat(32),
    )
    .unwrap()
}

#[test]
fn create_update_cancel_are_authorized_revision_checked_and_atomic() {
    let directory = tempdir().unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("management.sqlite3")).unwrap();
    let session = SessionId::try_from("session").unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let authority = Authority {
        allowed: Cell::new(true),
        operations: RefCell::new(Vec::new()),
    };
    let first = intent("revision-1", "2026-08-29T00:00:00Z");
    create_schedule(
        &mut ledger,
        &session,
        1,
        "create",
        &first,
        "2026-08-29T00:00:00Z",
        &authority,
    )
    .unwrap();

    let replacement = intent("revision-2", "2026-08-30T00:00:00Z");
    assert_eq!(
        update_schedule(
            &mut ledger,
            &session,
            2,
            "bad-update",
            "wrong-revision",
            &replacement,
            "2026-08-29T00:00:01Z",
            &authority,
        ),
        Err(ScheduleErrorCode::RevisionConflict)
    );
    update_schedule(
        &mut ledger,
        &session,
        2,
        "update",
        "revision-1",
        &replacement,
        "2026-08-29T00:00:01Z",
        &authority,
    )
    .unwrap();
    let state = reconstruct_schedule_state(&ledger, &session, "schedule-1").unwrap();
    assert_eq!(state.intent.revision_id(), "revision-2");
    assert!(state.active);

    authority.allowed.set(false);
    assert_eq!(
        cancel_schedule(
            &mut ledger,
            &session,
            3,
            "denied-cancel",
            "schedule-1",
            "revision-2",
            ScheduleCancelReason::User,
            "2026-08-29T00:00:02Z",
            &authority,
        ),
        Err(ScheduleErrorCode::AuthorityDenied)
    );
    assert!(
        reconstruct_schedule_state(&ledger, &session, "schedule-1")
            .unwrap()
            .active
    );
    authority.allowed.set(true);
    cancel_schedule(
        &mut ledger,
        &session,
        3,
        "cancel",
        "schedule-1",
        "revision-2",
        ScheduleCancelReason::User,
        "2026-08-29T00:00:02Z",
        &authority,
    )
    .unwrap();
    assert!(
        !reconstruct_schedule_state(&ledger, &session, "schedule-1")
            .unwrap()
            .active
    );
    assert_eq!(
        authority.operations.borrow().as_slice(),
        [
            ScheduleAuthorityOperation::Create,
            ScheduleAuthorityOperation::Update,
            ScheduleAuthorityOperation::Update,
            ScheduleAuthorityOperation::Cancel,
            ScheduleAuthorityOperation::Cancel,
        ]
    );
}
