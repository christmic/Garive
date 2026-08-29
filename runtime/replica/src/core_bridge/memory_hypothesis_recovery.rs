use std::collections::{BTreeMap, BTreeSet};

use garive_ledger::{DurableFact, FactKind};
use garive_memory::{
    DurableFactReference, EvidenceTally, HypothesisState, MemoryErrorCode, MemoryLifecycle,
    MemoryObligation,
};
use serde_json::{Map, Value};

use crate::SqliteLedger;

use super::memory_recovery::MemoryPrefix;

/// Redacted durable binding for one committed M1 recall.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordedMemoryRecall {
    /// Stable selection identity.
    pub selection_id: String,
    /// Digest over complete request semantics.
    pub request_digest: String,
    /// Whether the product was menu or detail.
    pub product: String,
    /// Exact source prefix.
    pub through_position: u64,
    /// Ordered selected record/revision identities.
    pub items: Vec<(String, String)>,
    /// Whether an eligible suffix was omitted.
    pub truncated: bool,
}

/// Namespace-isolated M1 projection rebuilt solely from ledger prefixes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryHypothesisProjection {
    namespace_id: String,
    lifecycles: BTreeMap<(String, String), MemoryLifecycle>,
    open_obligations: BTreeMap<String, MemoryObligation>,
    recalls: Vec<RecordedMemoryRecall>,
}

impl MemoryHypothesisProjection {
    /// Returns the isolated namespace.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the latest exact lifecycle for a revision.
    pub fn lifecycle(&self, record_id: &str, revision_id: &str) -> Option<&MemoryLifecycle> {
        self.lifecycles.get(&(record_id.into(), revision_id.into()))
    }
    /// Returns an obligation not yet closed by an atomic observation transition.
    pub fn open_obligation(&self, obligation_id: &str) -> Option<&MemoryObligation> {
        self.open_obligations.get(obligation_id)
    }
    /// Returns committed recalls in authorized prefix order.
    pub fn recalls(&self) -> &[RecordedMemoryRecall] {
        &self.recalls
    }
}

#[derive(Clone)]
struct PendingObservation {
    obligation_id: String,
    semantic_position: u64,
    session_id: String,
    durable_position: u64,
}

#[derive(Clone)]
struct Transition {
    record_id: String,
    revision_id: String,
    from_state: HypothesisState,
    lifecycle: MemoryLifecycle,
    cause_kind: String,
    cause_id: String,
    session_id: String,
    durable_position: u64,
}

/// Rebuilds M1 state from authorized fixed prefixes with hard namespace filtering.
pub fn reconstruct_memory_hypothesis_projection(
    ledger: &SqliteLedger,
    prefixes: &[MemoryPrefix],
    namespace_id: &str,
) -> Result<MemoryHypothesisProjection, MemoryErrorCode> {
    if namespace_id.is_empty()
        || prefixes.is_empty()
        || prefixes.iter().any(|item| item.through_position == 0)
        || !prefixes
            .windows(2)
            .all(|pair| pair[0].session_id < pair[1].session_id)
    {
        return Err(MemoryErrorCode::InvalidMemory);
    }
    let kinds = hypothesis_kinds();
    let mut facts = Vec::new();
    for prefix in prefixes {
        facts.extend(
            ledger
                .read_facts(&prefix.session_id, 0, prefix.through_position, Some(&kinds))
                .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        );
    }
    let mut obligations = BTreeMap::new();
    let mut observations = BTreeMap::new();
    let mut transitions = Vec::new();
    let mut recalls = Vec::new();
    for fact in facts {
        let payload: Value = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| MemoryErrorCode::CorruptMemoryState)?;
        let object = payload
            .as_object()
            .ok_or(MemoryErrorCode::CorruptMemoryState)?;
        if text(object, "namespace_id")? != namespace_id {
            continue;
        }
        match fact.kind.as_str() {
            "memory.recall_recorded" => recalls.push(parse_recall(object)?),
            "memory.obligation_opened" => {
                let obligation = parse_obligation(object)?;
                if obligations
                    .insert(obligation.obligation_id().to_owned(), obligation)
                    .is_some()
                {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
            }
            "memory.observation_recorded" => {
                let id = text(object, "observation_id")?;
                let pending = PendingObservation {
                    obligation_id: text(object, "obligation_id")?,
                    semantic_position: number(object, "position")?,
                    session_id: fact.session_id.as_str().into(),
                    durable_position: fact.position,
                };
                if observations.insert(id, pending).is_some() {
                    return Err(MemoryErrorCode::CorruptMemoryState);
                }
            }
            "memory.lifecycle_transitioned" => transitions.push(parse_transition(&fact, object)?),
            _ => return Err(MemoryErrorCode::CorruptMemoryState),
        }
    }
    transitions.sort_by_key(|item| {
        (
            item.record_id.clone(),
            item.revision_id.clone(),
            item.lifecycle.last_observed_position(),
        )
    });
    let mut lifecycles: BTreeMap<(String, String), MemoryLifecycle> = BTreeMap::new();
    let mut matched = BTreeSet::new();
    for transition in transitions {
        if transition.cause_kind == "observation" {
            let observed = observations
                .get(&transition.cause_id)
                .ok_or(MemoryErrorCode::CorruptMemoryState)?;
            let obligation = obligations
                .get(&observed.obligation_id)
                .ok_or(MemoryErrorCode::CorruptMemoryState)?;
            if observed.session_id != transition.session_id
                || observed.durable_position.checked_add(1) != Some(transition.durable_position)
                || observed.semantic_position != transition.lifecycle.last_observed_position()
                || obligation.record_id() != transition.record_id
                || obligation.revision_id() != transition.revision_id
                || !matched.insert(transition.cause_id.clone())
            {
                return Err(MemoryErrorCode::CorruptMemoryState);
            }
        }
        let key = (transition.record_id.clone(), transition.revision_id.clone());
        if let Some(prior) = lifecycles.get(&key) {
            if prior.state() != transition.from_state
                || prior.last_observed_position() >= transition.lifecycle.last_observed_position()
            {
                return Err(MemoryErrorCode::CorruptMemoryState);
            }
        }
        lifecycles.insert(key, transition.lifecycle);
    }
    if matched.len() != observations.len() {
        return Err(MemoryErrorCode::CorruptMemoryState);
    }
    for observation in observations.values() {
        obligations.remove(&observation.obligation_id);
    }
    Ok(MemoryHypothesisProjection {
        namespace_id: namespace_id.into(),
        lifecycles,
        open_obligations: obligations,
        recalls,
    })
}

fn parse_recall(value: &Map<String, Value>) -> Result<RecordedMemoryRecall, MemoryErrorCode> {
    let items = value["items"]
        .as_array()
        .ok_or(MemoryErrorCode::CorruptMemoryState)?
        .iter()
        .map(|item| {
            let item = item
                .as_object()
                .ok_or(MemoryErrorCode::CorruptMemoryState)?;
            Ok((text(item, "record_id")?, text(item, "revision_id")?))
        })
        .collect::<Result<Vec<_>, MemoryErrorCode>>()?;
    Ok(RecordedMemoryRecall {
        selection_id: text(value, "selection_id")?,
        request_digest: text(value, "request_digest")?,
        product: text(value, "product")?,
        through_position: number(value, "through_position")?,
        items,
        truncated: value["truncated"]
            .as_bool()
            .ok_or(MemoryErrorCode::CorruptMemoryState)?,
    })
}

fn parse_obligation(value: &Map<String, Value>) -> Result<MemoryObligation, MemoryErrorCode> {
    let fact = value["application_fact"]
        .as_object()
        .ok_or(MemoryErrorCode::CorruptMemoryState)?;
    MemoryObligation::new(
        text(value, "obligation_id")?,
        text(value, "record_id")?,
        text(value, "revision_id")?,
        DurableFactReference::new(
            text(fact, "session_id")?,
            number(fact, "position")?,
            text(fact, "fact_id")?,
            text(fact, "payload_digest")?,
        )
        .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        text(value, "expected_outcome_digest")?,
        text(value, "application_scope_digest")?,
        text(value, "attribution_policy_revision")?,
        number(value, "expires_at_position")?,
    )
    .map_err(|_| MemoryErrorCode::CorruptMemoryState)
}

fn parse_transition(
    fact: &DurableFact,
    value: &Map<String, Value>,
) -> Result<Transition, MemoryErrorCode> {
    let receipt = value
        .get("promoted_knowledge_receipt_digest")
        .map(|_| text(value, "promoted_knowledge_receipt_digest"))
        .transpose()?;
    Ok(Transition {
        record_id: text(value, "record_id")?,
        revision_id: text(value, "revision_id")?,
        from_state: parse_state(&text(value, "from_state")?)?,
        lifecycle: MemoryLifecycle::new(
            parse_state(&text(value, "to_state")?)?,
            EvidenceTally {
                verified: number(value, "verified")?,
                falsified: number(value, "falsified")?,
                neutral: number(value, "neutral")?,
            },
            number(value, "last_observed_position")?,
            receipt,
        )
        .map_err(|_| MemoryErrorCode::CorruptMemoryState)?,
        cause_kind: text(value, "cause_kind")?,
        cause_id: text(value, "cause_id")?,
        session_id: fact.session_id.as_str().into(),
        durable_position: fact.position,
    })
}

fn parse_state(value: &str) -> Result<HypothesisState, MemoryErrorCode> {
    match value {
        "candidate" => Ok(HypothesisState::Candidate),
        "active" => Ok(HypothesisState::Active),
        "cold" => Ok(HypothesisState::Cold),
        "archived" => Ok(HypothesisState::Archived),
        "promoted" => Ok(HypothesisState::Promoted),
        _ => Err(MemoryErrorCode::CorruptMemoryState),
    }
}
fn text(value: &Map<String, Value>, key: &str) -> Result<String, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn number(value: &Map<String, Value>, key: &str) -> Result<u64, MemoryErrorCode> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(MemoryErrorCode::CorruptMemoryState)
}
fn hypothesis_kinds() -> BTreeSet<FactKind> {
    [
        "memory.recall_recorded",
        "memory.obligation_opened",
        "memory.observation_recorded",
        "memory.lifecycle_transitioned",
    ]
    .into_iter()
    .map(|value| FactKind::new(value).expect("constant kind"))
    .collect()
}
