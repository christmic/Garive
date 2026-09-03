//! SQLite persistence for the Agent registry.

use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    validate_active_binding, validate_create_request, validate_knowledge_update, AgentStatus,
    CreateAgentRequest, RegisteredAgent, RegisteredAgentPage, UpdateAgentKnowledgeRequest,
};

const SELECT_AGENT: &str = "agent_id, working_directory, \
    readonly_knowledge_directories_json, writable_knowledge_directory, status";

/// Durable Agent registry command failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentRegistryError {
    /// Request fields or directory binding are invalid.
    InvalidRequest,
    /// Requested Agent does not exist.
    NotFound,
    /// Command identity was reused with different semantics.
    CommandConflict,
    /// Agent lifecycle or resources do not admit the operation.
    PreconditionFailed,
    /// SQLite could not durably complete the operation.
    StorageFailed,
    /// Persisted Agent metadata failed reconstruction.
    CorruptState,
}

/// SQLite-backed durable Agent metadata registry.
pub struct AgentRegistryStore<'a> {
    connection: &'a mut Connection,
}

impl<'a> AgentRegistryStore<'a> {
    /// Borrows a migrated Runtime SQLite connection.
    pub fn new(connection: &'a mut Connection) -> Self {
        Self { connection }
    }

    /// Creates one inactive Agent or exactly replays the same command.
    pub fn create(
        &mut self,
        command_id: &str,
        request: &CreateAgentRequest,
    ) -> Result<RegisteredAgent, AgentRegistryError> {
        validate_command_id(command_id)?;
        let request =
            validate_create_request(request).map_err(|_| AgentRegistryError::InvalidRequest)?;
        let digest = request_digest("create", &request)?;
        let transaction = self.transaction()?;
        if let Some(replayed) = replay(&transaction, command_id, "create", &digest)? {
            return finish_replay(transaction, replayed);
        }
        if load(&transaction, &request.agent_id)?.is_some() {
            return Err(AgentRegistryError::CommandConflict);
        }
        let agent = RegisteredAgent {
            api_version: "v1".into(),
            agent_id: request.agent_id,
            working_directory: request.working_directory,
            readonly_knowledge_directories: request.readonly_knowledge_directories,
            writable_knowledge_directory: request.writable_knowledge_directory,
            status: AgentStatus::Inactive,
        };
        insert_agent(&transaction, &agent)?;
        record_command(&transaction, command_id, "create", &digest, &agent)?;
        transaction
            .commit()
            .map_err(|_| AgentRegistryError::StorageFailed)?;
        Ok(agent)
    }

    /// Reads one exact Agent.
    pub fn get(&self, agent_id: &str) -> Result<RegisteredAgent, AgentRegistryError> {
        load(self.connection, agent_id)?.ok_or(AgentRegistryError::NotFound)
    }

    /// Lists a bounded registry page in stable identity order.
    pub fn list(&self, limit: usize) -> Result<RegisteredAgentPage, AgentRegistryError> {
        if limit == 0 {
            return Err(AgentRegistryError::InvalidRequest);
        }
        let sql =
            format!("SELECT {SELECT_AGENT} FROM registered_agents ORDER BY agent_id LIMIT ?1");
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|_| AgentRegistryError::StorageFailed)?;
        let rows = statement
            .query_map([limit as i64], row_to_agent)
            .map_err(|_| AgentRegistryError::StorageFailed)?;
        let agents = rows
            .map(|row| row.map_err(|_| AgentRegistryError::CorruptState))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RegisteredAgentPage {
            api_version: "v1",
            agents,
        })
    }

    /// Atomically replaces one Agent's mutable knowledge binding.
    pub fn update_knowledge(
        &mut self,
        command_id: &str,
        agent_id: &str,
        request: &UpdateAgentKnowledgeRequest,
    ) -> Result<RegisteredAgent, AgentRegistryError> {
        validate_command_id(command_id)?;
        let current = self.get(agent_id)?;
        let request = validate_knowledge_update(&current.working_directory, request)
            .map_err(|_| AgentRegistryError::InvalidRequest)?;
        let digest = request_digest("update_knowledge", &(agent_id, &request))?;
        let transaction = self.transaction()?;
        if let Some(replayed) = replay(&transaction, command_id, "update_knowledge", &digest)? {
            return finish_replay(transaction, replayed);
        }
        let mut agent = load(&transaction, agent_id)?.ok_or(AgentRegistryError::NotFound)?;
        agent.readonly_knowledge_directories = request.readonly_knowledge_directories;
        agent.writable_knowledge_directory = request.writable_knowledge_directory;
        if agent.status == AgentStatus::Active {
            validate_active_binding(&agent).map_err(|_| AgentRegistryError::PreconditionFailed)?;
        }
        update_agent(&transaction, &agent)?;
        record_command(
            &transaction,
            command_id,
            "update_knowledge",
            &digest,
            &agent,
        )?;
        transaction
            .commit()
            .map_err(|_| AgentRegistryError::StorageFailed)?;
        Ok(agent)
    }

    /// Validates and activates one Agent.
    pub fn activate(
        &mut self,
        command_id: &str,
        agent_id: &str,
    ) -> Result<RegisteredAgent, AgentRegistryError> {
        self.transition(command_id, agent_id, AgentStatus::Active, "activate", true)
    }

    /// Archives one Agent without disturbing already executing work.
    pub fn archive(
        &mut self,
        command_id: &str,
        agent_id: &str,
    ) -> Result<RegisteredAgent, AgentRegistryError> {
        self.transition(
            command_id,
            agent_id,
            AgentStatus::Archived,
            "archive",
            false,
        )
    }

    /// Returns one currently active and freshly validated Agent binding.
    pub fn require_active(&self, agent_id: &str) -> Result<RegisteredAgent, AgentRegistryError> {
        let agent = self.get(agent_id)?;
        if agent.status != AgentStatus::Active {
            return Err(AgentRegistryError::PreconditionFailed);
        }
        validate_active_binding(&agent).map_err(|_| AgentRegistryError::PreconditionFailed)?;
        Ok(agent)
    }

    fn transition(
        &mut self,
        command_id: &str,
        agent_id: &str,
        status: AgentStatus,
        operation: &'static str,
        validate: bool,
    ) -> Result<RegisteredAgent, AgentRegistryError> {
        validate_command_id(command_id)?;
        let digest = request_digest(operation, &agent_id)?;
        let transaction = self.transaction()?;
        if let Some(replayed) = replay(&transaction, command_id, operation, &digest)? {
            return finish_replay(transaction, replayed);
        }
        let mut agent = load(&transaction, agent_id)?.ok_or(AgentRegistryError::NotFound)?;
        if validate {
            validate_active_binding(&agent).map_err(|_| AgentRegistryError::PreconditionFailed)?;
        }
        agent.status = status;
        update_agent(&transaction, &agent)?;
        record_command(&transaction, command_id, operation, &digest, &agent)?;
        transaction
            .commit()
            .map_err(|_| AgentRegistryError::StorageFailed)?;
        Ok(agent)
    }

    fn transaction(&mut self) -> Result<Transaction<'_>, AgentRegistryError> {
        self.connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| AgentRegistryError::StorageFailed)
    }
}

fn load(
    connection: &Connection,
    agent_id: &str,
) -> Result<Option<RegisteredAgent>, AgentRegistryError> {
    let sql = format!("SELECT {SELECT_AGENT} FROM registered_agents WHERE agent_id = ?1");
    connection
        .query_row(&sql, [agent_id], row_to_agent)
        .optional()
        .map_err(|_| AgentRegistryError::CorruptState)
}

fn row_to_agent(row: &rusqlite::Row<'_>) -> rusqlite::Result<RegisteredAgent> {
    let readonly_json: String = row.get(2)?;
    let status: String = row.get(4)?;
    Ok(RegisteredAgent {
        api_version: "v1".into(),
        agent_id: row.get(0)?,
        working_directory: PathBuf::from(row.get::<_, String>(1)?),
        readonly_knowledge_directories: serde_json::from_str(&readonly_json)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        writable_knowledge_directory: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
        status: match status.as_str() {
            "inactive" => AgentStatus::Inactive,
            "active" => AgentStatus::Active,
            "archived" => AgentStatus::Archived,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
    })
}

fn insert_agent(
    transaction: &Transaction<'_>,
    agent: &RegisteredAgent,
) -> Result<(), AgentRegistryError> {
    let readonly = serde_json::to_string(&agent.readonly_knowledge_directories)
        .map_err(|_| AgentRegistryError::InvalidRequest)?;
    transaction.execute(
        "INSERT INTO registered_agents(agent_id, working_directory, readonly_knowledge_directories_json, writable_knowledge_directory, status) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![agent.agent_id, path_text(&agent.working_directory)?, readonly, optional_path_text(agent.writable_knowledge_directory.as_deref())?, status_text(agent.status)],
    ).map_err(|_| AgentRegistryError::StorageFailed)?;
    Ok(())
}

fn update_agent(
    transaction: &Transaction<'_>,
    agent: &RegisteredAgent,
) -> Result<(), AgentRegistryError> {
    let readonly = serde_json::to_string(&agent.readonly_knowledge_directories)
        .map_err(|_| AgentRegistryError::InvalidRequest)?;
    transaction.execute(
        "UPDATE registered_agents SET readonly_knowledge_directories_json=?2, writable_knowledge_directory=?3, status=?4 WHERE agent_id=?1",
        params![agent.agent_id, readonly, optional_path_text(agent.writable_knowledge_directory.as_deref())?, status_text(agent.status)],
    ).map_err(|_| AgentRegistryError::StorageFailed)?;
    Ok(())
}

fn replay(
    transaction: &Transaction<'_>,
    command_id: &str,
    operation: &str,
    digest: &str,
) -> Result<Option<RegisteredAgent>, AgentRegistryError> {
    let found = transaction.query_row(
        "SELECT operation, request_digest, response_json FROM agent_registry_commands WHERE command_id=?1",
        [command_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
    ).optional().map_err(|_| AgentRegistryError::StorageFailed)?;
    match found {
        None => Ok(None),
        Some((found_operation, found_digest, response))
            if found_operation == operation && found_digest == digest =>
        {
            serde_json::from_str(&response)
                .map(Some)
                .map_err(|_| AgentRegistryError::CorruptState)
        }
        Some(_) => Err(AgentRegistryError::CommandConflict),
    }
}

fn record_command(
    transaction: &Transaction<'_>,
    command_id: &str,
    operation: &str,
    digest: &str,
    agent: &RegisteredAgent,
) -> Result<(), AgentRegistryError> {
    let response = serde_json::to_string(agent).map_err(|_| AgentRegistryError::CorruptState)?;
    transaction.execute(
        "INSERT INTO agent_registry_commands(command_id, agent_id, operation, request_digest, response_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![command_id, agent.agent_id, operation, digest, response],
    ).map_err(|_| AgentRegistryError::StorageFailed)?;
    Ok(())
}

fn finish_replay(
    transaction: Transaction<'_>,
    agent: RegisteredAgent,
) -> Result<RegisteredAgent, AgentRegistryError> {
    transaction
        .commit()
        .map_err(|_| AgentRegistryError::StorageFailed)?;
    Ok(agent)
}

fn request_digest(operation: &str, request: &impl Serialize) -> Result<String, AgentRegistryError> {
    let bytes =
        serde_jcs::to_vec(&(operation, request)).map_err(|_| AgentRegistryError::InvalidRequest)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_command_id(value: &str) -> Result<(), AgentRegistryError> {
    if !(1..=128).contains(&value.len()) || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(AgentRegistryError::InvalidRequest);
    }
    Ok(())
}

fn path_text(path: &std::path::Path) -> Result<&str, AgentRegistryError> {
    path.to_str().ok_or(AgentRegistryError::InvalidRequest)
}

fn optional_path_text(path: Option<&std::path::Path>) -> Result<Option<&str>, AgentRegistryError> {
    path.map(path_text).transpose()
}

const fn status_text(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Inactive => "inactive",
        AgentStatus::Active => "active",
        AgentStatus::Archived => "archived",
    }
}
