//! Tool-capable execution ports layered above the C3 model-only boundary.

use std::{future::Future, pin::Pin};

use garive_tools::{
    GovernedToolResult, PreparationError, PreparedToolCall, ToolDefinition, ToolIntent,
};

use crate::{AgentExecutionPorts, AgentTurnRequest, ExecutionReport, PortFailure};

/// Immutable exact tool capabilities frozen for one Kernel Execution.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentToolCapabilities {
    /// Full C4 definitions resolved into the Effective Agent Snapshot.
    pub definitions: Vec<ToolDefinition>,
}

/// Governed result returned only after its required facts are durable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedGovernedResult {
    /// Portable C5 result visible to Core.
    pub result: GovernedToolResult,
    /// Latest committed Session position after this result.
    pub through_position: u64,
}

/// Asynchronous result for one governed Runtime operation.
pub type GovernedEffectFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CommittedGovernedResult, PortFailure>> + Send + 'a>>;

/// Runtime-owned durable authority/execution boundary used by the Agent loop.
pub trait GovernedEffectPort: Send {
    /// Commits a C4 preparation rejection before returning model feedback.
    fn reject<'a>(
        &'a mut self,
        source_model_request_id: &'a str,
        intent: &'a ToolIntent,
        error: &'a PreparationError,
    ) -> GovernedEffectFuture<'a>;

    /// Allocates, authorizes and executes or suspends one Prepared Call.
    fn invoke<'a>(
        &'a mut self,
        source_model_request_id: &'a str,
        prepared: &'a PreparedToolCall,
    ) -> GovernedEffectFuture<'a>;
}

/// Runs the C0-C5 tool-capable bounded Agent loop.
pub async fn execute_agent(
    request: &AgentTurnRequest,
    capabilities: &AgentToolCapabilities,
    ports: &mut AgentExecutionPorts<'_>,
    effects: &mut dyn GovernedEffectPort,
) -> ExecutionReport {
    crate::model_only::execute_with_tools(request, ports, &capabilities.definitions, effects).await
}
