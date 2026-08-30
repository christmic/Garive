//! Governed `ExecutorPort` bridge for T2 native adapters.

use std::collections::BTreeMap;

use garive_ledger::CanonicalPayload;
use garive_tools::{
    BuiltinT2BrowserCatalogue, BuiltinT2ComputerCatalogue, EffectReceipt, ExecutionCapability,
    ExecutionFact, InvocationGrant, PreparedToolCall, ReceiptId, ReplayClass,
    TerminalClassification, ToolIntent, ToolInvocationId, T2_BROWSER_ACT, T2_BROWSER_NAVIGATE,
    T2_BROWSER_OBSERVE, T2_COMPUTER_ACT, T2_COMPUTER_OBSERVE,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    ApplicationId, BrowserPageId, BrowserSessionId, DesktopSessionId, ExecutorDispatch,
    ExecutorDispatchError, ExecutorFuture, ExecutorPort, NativeActionCommandV1,
    NativeAdapterBindingV1, NativeAdapterPort, NativeObservationBounds, NativeProtocolError,
    NativeSnapshotId, NativeTarget, PreparedExecution, WindowId,
};

/// Stable executor identity used by T2 sandbox bindings.
pub const T2_NATIVE_EXECUTOR_ID: &str = "garive.native.capability";

/// Bridges exact Engine T2 calls into one narrow platform-native adapter.
pub struct NativeCapabilityExecutor<A> {
    revision: String,
    browser: BuiltinT2BrowserCatalogue,
    computer: BuiltinT2ComputerCatalogue,
    adapter: A,
    pending: BTreeMap<String, PendingAction>,
}

impl<A: NativeAdapterPort> NativeCapabilityExecutor<A> {
    /// Freezes the adapter revision and exact Browser/Computer catalogues.
    pub fn new(
        revision: impl Into<String>,
        browser: BuiltinT2BrowserCatalogue,
        computer: BuiltinT2ComputerCatalogue,
        adapter: A,
    ) -> Result<Self, NativeProtocolError> {
        let revision = revision.into();
        if revision.is_empty() {
            return Err(NativeProtocolError::InvalidBinding);
        }
        Ok(Self {
            revision,
            browser,
            computer,
            adapter,
            pending: BTreeMap::new(),
        })
    }
}

impl<A: NativeAdapterPort> ExecutorPort for NativeCapabilityExecutor<A> {
    fn prepare(
        &mut self,
        invocation_id: &ToolInvocationId,
        prepared: &PreparedToolCall,
        grant: &InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        let operation = operation(
            &self.browser,
            &self.computer,
            invocation_id,
            prepared,
            grant,
        )
        .map_err(|error| error.code().to_owned())?;
        let dispatch_attempt_id = dispatch_id(invocation_id);
        if let NativeOperation::Action(command) = operation {
            if self.pending.contains_key(&dispatch_attempt_id) {
                return Err(NativeProtocolError::InvalidBinding.code().into());
            }
            let binding = self
                .adapter
                .preflight_action(&command)
                .map_err(|error| error.code().to_owned())?;
            binding
                .validate()
                .map_err(|error| error.code().to_owned())?;
            self.pending.insert(
                dispatch_attempt_id.clone(),
                PendingAction { command, binding },
            );
        }
        Ok(PreparedExecution {
            executor_id: T2_NATIVE_EXECUTOR_ID.into(),
            executor_revision: self.revision.clone(),
            dispatch_attempt_id,
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        let operation = operation(
            &self.browser,
            &self.computer,
            command.invocation_id,
            command.prepared,
            command.grant,
        );
        let expected_dispatch = dispatch_id(command.invocation_id);
        Box::pin(async move {
            if command.execution.executor_id != T2_NATIVE_EXECUTOR_ID
                || command.execution.executor_revision != self.revision
                || command.execution.dispatch_attempt_id != expected_dispatch
            {
                return Err(ExecutorDispatchError::ReceiptInvalid);
            }
            match operation.map_err(|_| ExecutorDispatchError::ReceiptInvalid)? {
                NativeOperation::Observe {
                    target,
                    expected_previous_snapshot_id,
                    bounds,
                } => match self
                    .adapter
                    .observe(&target, expected_previous_snapshot_id.as_ref(), bounds)
                    .await
                {
                    Ok(observation) => {
                        if observation.target != target || observation.validate().is_err() {
                            return Err(ExecutorDispatchError::ReceiptInvalid);
                        }
                        let content = serde_json::to_value(observation)
                            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
                        bounded_completion(&command, content)
                    }
                    Err(error) => native_failure(&command, error),
                },
                NativeOperation::Action(expected) => {
                    let pending = self
                        .pending
                        .remove(&expected_dispatch)
                        .ok_or(ExecutorDispatchError::ReceiptInvalid)?;
                    if pending.command != expected {
                        return Err(ExecutorDispatchError::ReceiptInvalid);
                    }
                    match self
                        .adapter
                        .dispatch_action(&pending.command, &pending.binding)
                        .await
                    {
                        Ok(receipt) => {
                            if receipt.validate().is_err()
                                || receipt.action_id != pending.command.action_id
                                || receipt.prior_snapshot_id != pending.command.expected_snapshot_id
                                || receipt.binding != pending.binding
                            {
                                return Err(ExecutorDispatchError::ReceiptInvalid);
                            }
                            let classification = receipt.terminal_classification.clone();
                            let content = serde_json::to_value(receipt)
                                .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
                            if classification == "completed" {
                                bounded_completion(&command, content)
                            } else {
                                failed(&command, "native_action_failed", Some(content))
                            }
                        }
                        Err(error) => native_failure(&command, error),
                    }
                }
            }
        })
    }
}

struct PendingAction {
    command: NativeActionCommandV1,
    binding: NativeAdapterBindingV1,
}

enum NativeOperation {
    Observe {
        target: NativeTarget,
        expected_previous_snapshot_id: Option<NativeSnapshotId>,
        bounds: NativeObservationBounds,
    },
    Action(NativeActionCommandV1),
}

fn operation(
    browser: &BuiltinT2BrowserCatalogue,
    computer: &BuiltinT2ComputerCatalogue,
    invocation_id: &ToolInvocationId,
    prepared: &PreparedToolCall,
    grant: &InvocationGrant,
) -> Result<NativeOperation, NativeProtocolError> {
    if prepared.contract_version() != 3
        || grant.invocation_id != *invocation_id
        || grant.prepared_digest != prepared.input_digest()
        || grant.tool_name != prepared.tool_name()
        || grant.tool_revision != prepared.tool_revision()
        || !requirements_cover(prepared, grant)
    {
        return Err(NativeProtocolError::InvalidBinding);
    }
    let arguments: Value = serde_json::from_str(prepared.normalized_arguments())
        .map_err(|_| NativeProtocolError::InvalidBinding)?;
    let reconstructed = match prepared.tool_name() {
        T2_BROWSER_OBSERVE | T2_BROWSER_NAVIGATE | T2_BROWSER_ACT => {
            browser.prepare(&ToolIntent::new(
                prepared.model_call_id(),
                prepared.tool_name(),
                prepared.normalized_arguments(),
            ))
        }
        T2_COMPUTER_OBSERVE | T2_COMPUTER_ACT => computer.prepare(&ToolIntent::new(
            prepared.model_call_id(),
            prepared.tool_name(),
            prepared.normalized_arguments(),
        )),
        _ => return Err(NativeProtocolError::InvalidBinding),
    }
    .map_err(|_| NativeProtocolError::InvalidBinding)?;
    if reconstructed != *prepared {
        return Err(NativeProtocolError::InvalidBinding);
    }
    let target = target(prepared.tool_name(), &arguments)?;
    if matches!(
        prepared.tool_name(),
        T2_BROWSER_OBSERVE | T2_COMPUTER_OBSERVE
    ) {
        return Ok(NativeOperation::Observe {
            target,
            expected_previous_snapshot_id: optional_id(
                &arguments,
                "expected_previous_snapshot_id",
            )?,
            bounds: NativeObservationBounds {
                max_nodes: number(&arguments, "max_nodes")?,
                max_text_bytes: number(&arguments, "max_text_bytes")?,
            },
        });
    }
    Ok(NativeOperation::Action(NativeActionCommandV1 {
        action_id: crate::NativeActionId::new(format!(
            "action-{:x}",
            Sha256::digest(invocation_id.as_str().as_bytes())
        ))?,
        target,
        expected_snapshot_id: required_id(&arguments, "expected_snapshot_id")?,
        target_revision: text(&arguments, "target_revision")?.into(),
        prepared_input: arguments,
    }))
}

fn target(tool: &str, value: &Value) -> Result<NativeTarget, NativeProtocolError> {
    if matches!(
        tool,
        T2_BROWSER_OBSERVE | T2_BROWSER_NAVIGATE | T2_BROWSER_ACT
    ) {
        Ok(NativeTarget::Browser {
            session_id: BrowserSessionId::new(text(value, "session_id")?)?,
            page_id: BrowserPageId::new(text(value, "page_id")?)?,
        })
    } else {
        Ok(NativeTarget::Computer {
            session_id: DesktopSessionId::new(text(value, "desktop_session_id")?)?,
            application_id: ApplicationId::new(text(value, "application_id")?)?,
            window_id: WindowId::new(text(value, "window_id")?)?,
        })
    }
}

fn requirements_cover(prepared: &PreparedToolCall, grant: &InvocationGrant) -> bool {
    let requested = prepared.requirements();
    let granted = &grant.granted_requirements;
    let native = requested.capabilities().any(|capability| {
        matches!(
            capability,
            ExecutionCapability::BrowserObserve
                | ExecutionCapability::BrowserAct
                | ExecutionCapability::ComputerObserve
                | ExecutionCapability::ComputerAct
        )
    });
    native
        && requested.capabilities().eq(granted.capabilities())
        && granted.max_duration_ms() <= requested.max_duration_ms()
        && granted.max_output_bytes() <= requested.max_output_bytes()
        && matches!(
            prepared.replay_class(),
            ReplayClass::ReadOnly | ReplayClass::NeverReplay
        )
}

fn dispatch_id(invocation_id: &ToolInvocationId) -> String {
    format!(
        "native-dispatch-{:x}",
        Sha256::digest(invocation_id.as_str().as_bytes())
    )
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, NativeProtocolError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or(NativeProtocolError::InvalidBinding)
}

fn number(value: &Value, field: &str) -> Result<u32, NativeProtocolError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or(NativeProtocolError::InvalidBinding)
}

fn optional_id(
    value: &Value,
    field: &str,
) -> Result<Option<NativeSnapshotId>, NativeProtocolError> {
    value
        .get(field)
        .map(|value| {
            value
                .as_str()
                .ok_or(NativeProtocolError::InvalidBinding)
                .and_then(NativeSnapshotId::new)
        })
        .transpose()
}

fn required_id(value: &Value, field: &str) -> Result<NativeSnapshotId, NativeProtocolError> {
    NativeSnapshotId::new(text(value, field)?)
}

fn native_failure(
    command: &ExecutorDispatch<'_>,
    error: NativeProtocolError,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    match error {
        NativeProtocolError::ActionUncertain => Err(ExecutorDispatchError::StartedWithoutReceipt),
        NativeProtocolError::ReceiptInvalid | NativeProtocolError::InvalidBinding => {
            Err(ExecutorDispatchError::ReceiptInvalid)
        }
        _ => failed(command, error.code(), None),
    }
}

fn completed(
    command: &ExecutorDispatch<'_>,
    content: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    bounded(command, &content)?;
    Ok(ExecutionFact::Completed {
        receipt: Some(effect_receipt(
            command,
            TerminalClassification::Completed,
            &content,
        )?),
        content,
        truncated: false,
    })
}

fn bounded_completion(
    command: &ExecutorDispatch<'_>,
    content: Value,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    if bounded(command, &content).is_err() {
        failed(
            command,
            NativeProtocolError::ResultBoundExceeded.code(),
            None,
        )
    } else {
        completed(command, content)
    }
}

fn failed(
    command: &ExecutorDispatch<'_>,
    code: &str,
    partial: Option<Value>,
) -> Result<ExecutionFact, ExecutorDispatchError> {
    let evidence = json!({"code":code,"details":null,"partial":partial});
    bounded(command, &evidence)?;
    Ok(ExecutionFact::Failed {
        receipt: Some(effect_receipt(
            command,
            TerminalClassification::Failed,
            &evidence,
        )?),
        code: code.into(),
        details: None,
        partial,
    })
}

fn bounded(command: &ExecutorDispatch<'_>, value: &Value) -> Result<(), ExecutorDispatchError> {
    let payload =
        CanonicalPayload::from_value(value).map_err(|_| ExecutorDispatchError::ReceiptInvalid)?;
    let bound = command
        .prepared
        .max_result_bytes()
        .ok_or(ExecutorDispatchError::ReceiptInvalid)?
        .min(command.grant.granted_requirements.max_output_bytes());
    if payload.as_json().len() as u64 > bound {
        Err(ExecutorDispatchError::ReceiptInvalid)
    } else {
        Ok(())
    }
}

fn effect_receipt(
    command: &ExecutorDispatch<'_>,
    classification: TerminalClassification,
    value: &Value,
) -> Result<EffectReceipt, ExecutorDispatchError> {
    let digest = CanonicalPayload::from_value(value)
        .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?
        .sha256()
        .to_owned();
    Ok(EffectReceipt {
        receipt_id: ReceiptId::new(command.receipt_id)
            .map_err(|_| ExecutorDispatchError::ReceiptInvalid)?,
        invocation_id: command.invocation_id.clone(),
        prepared_digest: command.prepared.input_digest().into(),
        grant_id: command.grant.grant_id.clone(),
        executor_id: command.execution.executor_id.clone(),
        executor_revision: command.execution.executor_revision.clone(),
        terminal_classification: classification,
        result_digest: digest,
    })
}
