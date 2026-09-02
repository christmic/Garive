use std::collections::BTreeSet;

use serde::Serialize;

use crate::{DelegationError, DelegationErrorCode};

/// Maximum number of stable named peers in one durable Session.
pub const MAX_NAMED_SESSION_AGENTS: usize = 10;
const MAX_DISPLAY_NAME_BYTES: usize = 64;

/// One stable named peer admitted to a Session roster.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NamedAgent {
    agent_instance_id: String,
    display_name: String,
}

impl NamedAgent {
    /// Validates one identity/display-name binding.
    pub fn new(
        agent_instance_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, DelegationError> {
        let value = Self {
            agent_instance_id: agent_instance_id.into(),
            display_name: display_name.into(),
        };
        if valid_text(&value.agent_instance_id, 128)
            && valid_text(&value.display_name, MAX_DISPLAY_NAME_BYTES)
        {
            Ok(value)
        } else {
            Err(invalid())
        }
    }

    /// Returns the Runtime Agent Instance identity.
    pub fn agent_instance_id(&self) -> &str {
        &self.agent_instance_id
    }

    /// Returns Session-unique product display metadata.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// Immutable validated named-member roster snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionRoster {
    members: Vec<NamedAgent>,
}

impl SessionRoster {
    /// Admits at most ten members with unique identities and exact names.
    pub fn new(members: Vec<NamedAgent>) -> Result<Self, DelegationError> {
        let mut identities = BTreeSet::new();
        let mut names = BTreeSet::new();
        if members.len() > MAX_NAMED_SESSION_AGENTS
            || members.iter().any(|member| {
                !identities.insert(member.agent_instance_id.clone())
                    || !names.insert(member.display_name.clone())
            })
        {
            return Err(invalid());
        }
        Ok(Self { members })
    }

    /// Returns members in stable roster order; order conveys no authority.
    pub fn members(&self) -> &[NamedAgent] {
        &self.members
    }

    /// Resolves one exact named member identity.
    pub fn contains(&self, agent_instance_id: &str) -> bool {
        self.members
            .iter()
            .any(|member| member.agent_instance_id == agent_instance_id)
    }
}

/// Runtime-resolved assignee requested by one temporary delegation edge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssigneeSelector {
    /// Allocate a task-scoped Agent not added to the named roster.
    Anonymous {
        /// Exact installed Agent Definition.
        definition_id: String,
        /// Exact immutable Definition revision.
        definition_revision: String,
    },
    /// Fork the dispatcher's own definition and a frozen Session prefix.
    ForkSelf {
        /// Dispatcher identity that must equal the command actor.
        source_agent_instance_id: String,
        /// Immutable Session prefix exposed to the fork.
        through_position: u64,
        /// Optional product label; never an Agent name or authority.
        #[serde(skip_serializing_if = "Option::is_none")]
        branch_name: Option<String>,
    },
    /// Address one existing equal named Session peer.
    Named {
        /// Exact roster Agent Instance identity.
        agent_instance_id: String,
    },
}

impl AssigneeSelector {
    /// Validates an anonymous definition binding.
    pub fn anonymous(
        definition_id: impl Into<String>,
        definition_revision: impl Into<String>,
    ) -> Result<Self, DelegationError> {
        let definition_id = definition_id.into();
        let definition_revision = definition_revision.into();
        if valid_text(&definition_id, 128) && valid_text(&definition_revision, 128) {
            Ok(Self::Anonymous {
                definition_id,
                definition_revision,
            })
        } else {
            Err(invalid())
        }
    }

    /// Validates a self-fork selector at a non-zero durable prefix.
    pub fn fork_self(
        source_agent_instance_id: impl Into<String>,
        through_position: u64,
        branch_name: Option<String>,
    ) -> Result<Self, DelegationError> {
        let source_agent_instance_id = source_agent_instance_id.into();
        if !valid_text(&source_agent_instance_id, 128)
            || through_position == 0
            || branch_name
                .as_deref()
                .is_some_and(|name| !valid_text(name, MAX_DISPLAY_NAME_BYTES))
        {
            return Err(invalid());
        }
        Ok(Self::ForkSelf {
            source_agent_instance_id,
            through_position,
            branch_name,
        })
    }

    /// Validates a named-peer selector and roster membership.
    pub fn named(
        agent_instance_id: impl Into<String>,
        roster: &SessionRoster,
    ) -> Result<Self, DelegationError> {
        let agent_instance_id = agent_instance_id.into();
        if roster.contains(&agent_instance_id) {
            Ok(Self::Named { agent_instance_id })
        } else {
            Err(invalid())
        }
    }
}

/// Result-delivery semantics selected independently from assignee identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPolicy {
    /// Deliver a durable addressed result without blocking the dispatcher.
    Notify,
    /// Let the dispatcher continue, but require an explicit join before final.
    AwaitBeforeFinal,
    /// Close the current Execution and resume it from the governed result.
    SuspendExecution,
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && value.trim() == value
}

const fn invalid() -> DelegationError {
    DelegationError::new(DelegationErrorCode::InvalidDelegation)
}
