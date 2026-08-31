//! Exact Tool-to-executor routing for one installed Agent snapshot.

use std::collections::BTreeMap;

use garive_tools::{EffectReceipt, InvocationGrant, PreparedToolCall, ToolInvocationId};

use crate::{
    ExecutorDispatch, ExecutorDispatchError, ExecutorFuture, ExecutorPort, ExecutorRecoveryRequest,
    PreparedExecution,
};

/// One executor plus the exact Tool names it implements.
pub struct ExecutorRoute {
    executor_id: String,
    tool_names: Vec<String>,
    executor: Box<dyn ExecutorPort>,
}

impl ExecutorRoute {
    /// Constructs one explicit non-empty executor route.
    pub fn new(
        executor_id: impl Into<String>,
        tool_names: impl IntoIterator<Item = impl Into<String>>,
        executor: Box<dyn ExecutorPort>,
    ) -> Result<Self, String> {
        let executor_id = executor_id.into();
        let mut tool_names = tool_names.into_iter().map(Into::into).collect::<Vec<_>>();
        tool_names.sort();
        if executor_id.is_empty()
            || tool_names.is_empty()
            || tool_names.iter().any(String::is_empty)
            || tool_names.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err("invalid executor route".into());
        }
        Ok(Self {
            executor_id,
            tool_names,
            executor,
        })
    }
}

/// Closed executor set that rejects unknown or inconsistently bound routes.
pub struct RoutedExecutorPort {
    tool_routes: BTreeMap<String, String>,
    executors: BTreeMap<String, Box<dyn ExecutorPort>>,
}

impl RoutedExecutorPort {
    /// Constructs a router with unique Tool names and executor identities.
    pub fn new(routes: impl IntoIterator<Item = ExecutorRoute>) -> Result<Self, String> {
        let mut tool_routes = BTreeMap::new();
        let mut executors = BTreeMap::new();
        for route in routes {
            if executors.contains_key(&route.executor_id) {
                return Err("duplicate executor identity".into());
            }
            for tool_name in &route.tool_names {
                if tool_routes
                    .insert(tool_name.clone(), route.executor_id.clone())
                    .is_some()
                {
                    return Err("duplicate Tool executor route".into());
                }
            }
            executors.insert(route.executor_id, route.executor);
        }
        if executors.is_empty() {
            return Err("empty executor router".into());
        }
        Ok(Self {
            tool_routes,
            executors,
        })
    }

    fn executor_for_tool(&mut self, tool_name: &str) -> Option<(&str, &mut dyn ExecutorPort)> {
        let executor_id = self.tool_routes.get(tool_name)?;
        let executor = self.executors.get_mut(executor_id)?;
        Some((executor_id, executor.as_mut()))
    }
}

impl ExecutorPort for RoutedExecutorPort {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        let (expected_id, executor) = self
            .executor_for_tool(prepared.tool_name())
            .ok_or_else(|| "Tool has no installed executor".to_owned())?;
        let execution = executor.prepare(invocation_id, prepared, grant)?;
        if execution.executor_id != expected_id {
            return Err("executor returned a different identity".into());
        }
        Ok(execution)
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        match self.executors.get_mut(&command.execution.executor_id) {
            Some(executor) => executor.dispatch(command),
            None => Box::pin(async { Err(ExecutorDispatchError::ReceiptInvalid) }),
        }
    }

    fn acknowledge_receipt(
        &mut self,
        invocation_id: &ToolInvocationId,
        receipt: &EffectReceipt,
    ) -> Result<(), ExecutorDispatchError> {
        self.executors
            .get_mut(&receipt.executor_id)
            .ok_or(ExecutorDispatchError::ReceiptInvalid)?
            .acknowledge_receipt(invocation_id, receipt)
    }

    fn reconcile_started_loss(
        &mut self,
        request: ExecutorRecoveryRequest<'_>,
    ) -> Result<(), ExecutorDispatchError> {
        self.executors
            .get_mut(request.executor_id)
            .ok_or(ExecutorDispatchError::ReceiptInvalid)?
            .reconcile_started_loss(request)
    }
}
