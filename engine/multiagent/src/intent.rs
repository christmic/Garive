use std::collections::BTreeSet;

use garive_tools::validate_portable_value_schema;
use serde::Serialize;
use serde_json::json;

use crate::values::{sha256, valid_id};
use crate::{
    ContentBinding, DelegationBudget, DelegationError, DelegationErrorCode, FactReference,
};

const INTENT_CONTRACT: &str = "garive.delegation-intent";
const CONTRACT_VERSION: u32 = 1;

/// Exact existing-child or definition requirement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChildRequirement {
    /// Delegate to one existing admitted child Agent instance.
    Existing {
        /// Exact child Agent instance identity.
        child_agent_instance_id: String,
    },
    /// Allocate a child bound to one exact definition revision.
    Definition {
        /// Agent definition identity.
        definition_id: String,
        /// Immutable definition revision.
        definition_revision: String,
    },
}

impl ChildRequirement {
    /// Validates an existing child identity.
    pub fn existing(child_agent_instance_id: impl Into<String>) -> Result<Self, DelegationError> {
        let child_agent_instance_id = child_agent_instance_id.into();
        if valid_id(&child_agent_instance_id) {
            Ok(Self::Existing {
                child_agent_instance_id,
            })
        } else {
            Err(invalid())
        }
    }
    /// Validates an exact requested child definition revision.
    pub fn definition(
        definition_id: impl Into<String>,
        definition_revision: impl Into<String>,
    ) -> Result<Self, DelegationError> {
        let definition_id = definition_id.into();
        let definition_revision = definition_revision.into();
        if valid_id(&definition_id) && valid_id(&definition_revision) {
            Ok(Self::Definition {
                definition_id,
                definition_revision,
            })
        } else {
            Err(invalid())
        }
    }
}

/// Parent-cancellation behavior for an already started child.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    /// Child lifecycle remains independently governed.
    Independent,
    /// Runtime commits a child cancel request after parent cancellation.
    CancelWithParent,
}

/// Canonical inline intent committed by `delegation.requested`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationIntentBinding {
    /// SHA-256 of the canonical intent bytes.
    pub digest: String,
    /// RFC 8785 canonical intent JSON.
    pub inline_utf8: String,
}

/// Immutable parent-to-child request semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationIntent {
    delegation_id: String,
    parent_agent_instance_id: String,
    parent_turn_id: String,
    parent_execution_id: String,
    child_requirement: ChildRequirement,
    objective: ContentBinding,
    input_evidence: Vec<FactReference>,
    result_schema: ContentBinding,
    budget: DelegationBudget,
    cancellation_policy: CancellationPolicy,
    through_position: u64,
}

impl DelegationIntent {
    /// Validates identities, exact evidence bounds, portable schema and budget.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        delegation_id: impl Into<String>,
        parent_agent_instance_id: impl Into<String>,
        parent_turn_id: impl Into<String>,
        parent_execution_id: impl Into<String>,
        child_requirement: ChildRequirement,
        objective: ContentBinding,
        input_evidence: Vec<FactReference>,
        result_schema: ContentBinding,
        budget: DelegationBudget,
        cancellation_policy: CancellationPolicy,
        through_position: u64,
    ) -> Result<Self, DelegationError> {
        budget.validate()?;
        let value = Self {
            delegation_id: delegation_id.into(),
            parent_agent_instance_id: parent_agent_instance_id.into(),
            parent_turn_id: parent_turn_id.into(),
            parent_execution_id: parent_execution_id.into(),
            child_requirement,
            objective,
            input_evidence,
            result_schema,
            budget,
            cancellation_policy,
            through_position,
        };
        if !valid_id(&value.delegation_id)
            || !valid_id(&value.parent_agent_instance_id)
            || !valid_id(&value.parent_turn_id)
            || !valid_id(&value.parent_execution_id)
            || value
                .objective
                .inline_utf8()
                .is_some_and(|text| text.len() as u64 > value.budget.max_objective_bytes)
            || value.input_evidence.len() as u64 > value.budget.max_input_evidence
            || !valid_evidence(&value.input_evidence, value.through_position)
            || !valid_schema(&value.result_schema, value.budget.max_result_schema_bytes)
        {
            return Err(invalid());
        }
        Ok(value)
    }

    /// Computes the canonical portable intent binding.
    pub fn intent_binding(&self) -> Result<DelegationIntentBinding, DelegationError> {
        let bytes = serde_jcs::to_vec(&self.intent_value()).map_err(|_| invalid())?;
        let inline_utf8 = String::from_utf8(bytes).map_err(|_| invalid())?;
        Ok(DelegationIntentBinding {
            digest: sha256(inline_utf8.as_bytes()),
            inline_utf8,
        })
    }
    /// Returns the canonical intent digest.
    pub fn intent_digest(&self) -> Result<String, DelegationError> {
        self.intent_binding().map(|binding| binding.digest)
    }
    /// Returns logical delegation identity.
    pub fn delegation_id(&self) -> &str {
        &self.delegation_id
    }
    /// Returns parent Agent instance identity.
    pub fn parent_agent_instance_id(&self) -> &str {
        &self.parent_agent_instance_id
    }
    /// Returns parent Turn identity.
    pub fn parent_turn_id(&self) -> &str {
        &self.parent_turn_id
    }
    /// Returns parent Execution identity.
    pub fn parent_execution_id(&self) -> &str {
        &self.parent_execution_id
    }
    /// Returns exact child requirement.
    pub const fn child_requirement(&self) -> &ChildRequirement {
        &self.child_requirement
    }
    /// Returns bounded objective binding.
    pub const fn objective(&self) -> &ContentBinding {
        &self.objective
    }
    /// Returns ordered durable input evidence.
    pub fn input_evidence(&self) -> &[FactReference] {
        &self.input_evidence
    }
    /// Returns canonical portable result schema binding.
    pub const fn result_schema(&self) -> &ContentBinding {
        &self.result_schema
    }
    /// Returns requested finite budget.
    pub const fn budget(&self) -> &DelegationBudget {
        &self.budget
    }
    /// Returns parent cancellation policy.
    pub const fn cancellation_policy(&self) -> CancellationPolicy {
        self.cancellation_policy
    }
    /// Returns fixed parent durable prefix.
    pub const fn through_position(&self) -> u64 {
        self.through_position
    }

    fn intent_value(&self) -> serde_json::Value {
        json!({
            "contract":INTENT_CONTRACT,"version":CONTRACT_VERSION,
            "parent_agent_instance_id":self.parent_agent_instance_id,
            "parent_turn_id":self.parent_turn_id,"parent_execution_id":self.parent_execution_id,
            "child_requirement":self.child_requirement,"objective":self.objective,
            "input_evidence":self.input_evidence,"result_schema":self.result_schema,
            "budget":self.budget,"cancellation_policy":self.cancellation_policy,
            "through_position":self.through_position,
        })
    }
}

fn valid_evidence(values: &[FactReference], through_position: u64) -> bool {
    let mut unique = BTreeSet::new();
    values
        .iter()
        .all(|value| value.position() <= through_position && unique.insert(value.clone()))
}

fn valid_schema(binding: &ContentBinding, maximum: u64) -> bool {
    let Some(inline) = binding.inline_utf8() else {
        return false;
    };
    if inline.len() as u64 > maximum {
        return false;
    }
    serde_json::from_str(inline).is_ok_and(|value| {
        serde_jcs::to_string(&value).is_ok_and(|canonical| canonical == inline)
            && validate_portable_value_schema(&value).is_ok()
    })
}

const fn invalid() -> DelegationError {
    DelegationError::new(DelegationErrorCode::InvalidDelegation)
}
