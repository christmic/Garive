use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::access::{AccessMode, InvocationAccessSet, ToolAccessPolicyV1, ToolAccessResolver};
use crate::sandbox::SandboxRequirementsV1;
use crate::schema::{parse_arguments, validate_arguments, validate_definition};

/// Stable failure classification for C4 preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationErrorCode {
    /// Model correlation identity is empty.
    InvalidModelCallId,
    /// Proposed tool name is empty.
    InvalidToolName,
    /// The frozen catalog does not admit the proposed tool.
    ToolNotAdmitted,
    /// Argument text is malformed, trailing, or contains duplicate keys.
    InvalidArgumentsJson,
    /// Parsed arguments do not satisfy the exact tool schema.
    ArgumentsSchemaMismatch,
    /// A tool definition or execution requirement is invalid.
    InvalidToolDefinition,
    /// The schema contains a keyword outside Portable Tool Schema v1.
    UnsupportedSchemaKeyword,
    /// A value cannot be represented by the admitted JCS surface.
    NonCanonicalValue,
    /// A C5b access declaration, key, resolver result, or policy is invalid.
    EffectAccessInvalid,
    /// An F0 portable sandbox requirement is malformed or incomplete.
    SandboxRequirementInvalid,
}

/// Deterministic JSON Schema assertion failure.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SchemaFailure {
    instance_path: String,
    schema_path: String,
    keyword: String,
}

impl SchemaFailure {
    pub(crate) fn new(instance_path: &str, schema_path: &str, keyword: &str) -> Self {
        Self {
            instance_path: instance_path.to_owned(),
            schema_path: schema_path.to_owned(),
            keyword: keyword.to_owned(),
        }
    }
    /// Returns the RFC 6901 path into the rejected instance.
    pub fn instance_path(&self) -> &str {
        &self.instance_path
    }
    /// Returns the RFC 6901 path into the exact schema.
    pub fn schema_path(&self) -> &str {
        &self.schema_path
    }
    /// Returns the stable failing keyword.
    pub fn keyword(&self) -> &str {
        &self.keyword
    }
}

/// Typed C4 construction or preparation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationError {
    code: PreparationErrorCode,
    failures: Vec<SchemaFailure>,
}

impl PreparationError {
    pub(crate) const fn new(code: PreparationErrorCode) -> Self {
        Self {
            code,
            failures: Vec::new(),
        }
    }
    pub(crate) const fn with_failures(
        code: PreparationErrorCode,
        failures: Vec<SchemaFailure>,
    ) -> Self {
        Self { code, failures }
    }
    /// Returns the stable failure classification.
    pub const fn code(&self) -> PreparationErrorCode {
        self.code
    }
    /// Returns deterministic schema failures, empty for non-schema failures.
    pub fn failures(&self) -> &[SchemaFailure] {
        &self.failures
    }
}

/// Neutral executor capability declared by one exact tool definition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionCapability {
    /// Read from an admitted filesystem surface.
    FilesystemRead,
    /// Mutate an admitted filesystem surface.
    FilesystemWrite,
    /// Start a bounded process.
    Process,
    /// Access an admitted network surface.
    Network,
    /// Observe one admitted browser session without mutation.
    BrowserObserve,
    /// Mutate one admitted browser session.
    BrowserAct,
    /// Observe one admitted native desktop target without input.
    ComputerObserve,
    /// Send bounded input to one admitted native desktop target.
    ComputerAct,
}

impl ExecutionCapability {
    /// Returns the stable portable capability name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::FilesystemRead => "filesystem_read",
            Self::FilesystemWrite => "filesystem_write",
            Self::Process => "process",
            Self::Network => "network",
            Self::BrowserObserve => "browser_observe",
            Self::BrowserAct => "browser_act",
            Self::ComputerObserve => "computer_observe",
            Self::ComputerAct => "computer_act",
        }
    }
}

/// Immutable executor requirements carried by a Prepared Call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutionRequirements {
    capabilities: BTreeSet<ExecutionCapability>,
    max_duration_ms: u64,
    max_output_bytes: u64,
}

impl ExecutionRequirements {
    /// Validates non-zero limits and unique capabilities.
    pub fn new(
        capabilities: impl IntoIterator<Item = ExecutionCapability>,
        max_duration_ms: u64,
        max_output_bytes: u64,
    ) -> Result<Self, PreparationError> {
        let values: Vec<_> = capabilities.into_iter().collect();
        let unique: BTreeSet<_> = values.iter().copied().collect();
        if max_duration_ms == 0 || max_output_bytes == 0 || unique.len() != values.len() {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidToolDefinition,
            ));
        }
        Ok(Self {
            capabilities: unique,
            max_duration_ms,
            max_output_bytes,
        })
    }
    /// Returns capabilities in their canonical enum order.
    pub fn capabilities(&self) -> impl ExactSizeIterator<Item = ExecutionCapability> + '_ {
        self.capabilities.iter().copied()
    }
    /// Returns the maximum admitted execution duration.
    pub const fn max_duration_ms(&self) -> u64 {
        self.max_duration_ms
    }
    /// Returns the maximum admitted model-visible output size.
    pub const fn max_output_bytes(&self) -> u64 {
        self.max_output_bytes
    }
}

/// Recovery safety declaration that Runtime must independently prove.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClass {
    /// Read-only operation eligible for a proven same-ID retry.
    ReadOnly,
    /// Executor supports a proven idempotency identity.
    Idempotent,
    /// Executor recovers from a committed receipt or journal.
    ReceiptRecoverable,
    /// An uncertain started operation always requires reconciliation.
    NeverReplay,
}

/// Exact immutable definition admitted to one execution snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolDefinition {
    name: String,
    revision: String,
    description: String,
    input_schema: Value,
    requirements: ExecutionRequirements,
    replay_class: ReplayClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_contract: Option<ToolAccessContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox_requirements: Option<SandboxRequirementsV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ToolAccessContract {
    policy: ToolAccessPolicyV1,
    resolver_revision: String,
}

impl ToolDefinition {
    /// Validates and constructs one Portable Tool Schema v1 definition.
    pub fn new(
        name: impl Into<String>,
        revision: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        requirements: ExecutionRequirements,
        replay_class: ReplayClass,
    ) -> Result<Self, PreparationError> {
        Self::new_internal(
            name,
            revision,
            description,
            input_schema,
            requirements,
            replay_class,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_internal(
        name: impl Into<String>,
        revision: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        requirements: ExecutionRequirements,
        replay_class: ReplayClass,
        v2_access_proof: bool,
    ) -> Result<Self, PreparationError> {
        let (name, revision, description) = (name.into(), revision.into(), description.into());
        if name.is_empty() || revision.is_empty() || description.is_empty() {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidToolDefinition,
            ));
        }
        validate_definition(&input_schema)?;
        if replay_class == ReplayClass::ReadOnly
            && requirements.capabilities().any(|capability| {
                !matches!(
                    capability,
                    ExecutionCapability::FilesystemRead
                        | ExecutionCapability::BrowserObserve
                        | ExecutionCapability::ComputerObserve
                ) && !(v2_access_proof && capability == ExecutionCapability::Network)
            })
        {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidToolDefinition,
            ));
        }
        Ok(Self {
            name,
            revision,
            description,
            input_schema,
            requirements,
            replay_class,
            access_contract: None,
            sandbox_requirements: None,
        })
    }

    /// Constructs a Prepared v2-capable definition with a frozen access contract.
    #[allow(clippy::too_many_arguments)]
    pub fn new_v2(
        name: impl Into<String>,
        revision: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        requirements: ExecutionRequirements,
        replay_class: ReplayClass,
        access_policy: ToolAccessPolicyV1,
        access_resolver_revision: impl Into<String>,
    ) -> Result<Self, PreparationError> {
        let mut definition = Self::new_internal(
            name,
            revision,
            description,
            input_schema,
            requirements,
            replay_class,
            true,
        )?;
        let resolver_revision = access_resolver_revision.into();
        if resolver_revision.is_empty() {
            return Err(PreparationError::new(
                PreparationErrorCode::EffectAccessInvalid,
            ));
        }
        definition.access_contract = Some(ToolAccessContract {
            policy: access_policy,
            resolver_revision,
        });
        Ok(definition)
    }

    /// Constructs a Prepared v3 definition with exact access and F0 controls.
    #[allow(clippy::too_many_arguments)]
    pub fn new_v3(
        name: impl Into<String>,
        revision: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        requirements: ExecutionRequirements,
        replay_class: ReplayClass,
        access_policy: ToolAccessPolicyV1,
        access_resolver_revision: impl Into<String>,
        sandbox_requirements: SandboxRequirementsV1,
    ) -> Result<Self, PreparationError> {
        sandbox_requirements.validate_for(requirements.capabilities())?;
        let mut definition = Self::new_v2(
            name,
            revision,
            description,
            input_schema,
            requirements,
            replay_class,
            access_policy,
            access_resolver_revision,
        )?;
        definition.sandbox_requirements = Some(sandbox_requirements);
        Ok(definition)
    }
    /// Returns the admitted provider-neutral name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the immutable definition revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }
    /// Returns the model-visible description.
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Returns the validated Portable Tool Schema v1 value.
    pub const fn input_schema(&self) -> &Value {
        &self.input_schema
    }
    /// Returns the declared execution requirements.
    pub const fn requirements(&self) -> &ExecutionRequirements {
        &self.requirements
    }
    /// Returns the declared recovery class.
    pub const fn replay_class(&self) -> ReplayClass {
        self.replay_class
    }

    /// Returns the immutable v2/v3 access ceiling, when installed.
    pub fn access_policy(&self) -> Option<&ToolAccessPolicyV1> {
        self.access_contract
            .as_ref()
            .map(|contract| &contract.policy)
    }

    /// Returns the immutable v3 sandbox enforcement request, when installed.
    pub const fn sandbox_requirements(&self) -> Option<&SandboxRequirementsV1> {
        self.sandbox_requirements.as_ref()
    }

    /// Returns the opted-in Prepared Call contract version.
    pub const fn prepared_contract_version(&self) -> u16 {
        if self.sandbox_requirements.is_some() {
            3
        } else if self.access_contract.is_some() {
            2
        } else {
            1
        }
    }
}

/// Untrusted model proposal supplied to C4.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIntent {
    model_call_id: String,
    tool_name: String,
    arguments_json: String,
}

impl ToolIntent {
    /// Constructs an untrusted proposal; validation happens during preparation.
    pub fn new(
        model_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments_json: impl Into<String>,
    ) -> Self {
        Self {
            model_call_id: model_call_id.into(),
            tool_name: tool_name.into(),
            arguments_json: arguments_json.into(),
        }
    }

    /// Returns untrusted model correlation.
    pub fn model_call_id(&self) -> &str {
        &self.model_call_id
    }

    /// Returns the proposed tool name.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }
}

/// Frozen exact-name catalog for one Kernel Execution.
#[derive(Clone, Debug)]
pub struct ToolCatalog {
    definitions: BTreeMap<String, ToolDefinition>,
}

impl ToolCatalog {
    /// Rejects duplicate names and constructs an immutable catalog.
    pub fn new(
        definitions: impl IntoIterator<Item = ToolDefinition>,
    ) -> Result<Self, PreparationError> {
        let mut catalog = BTreeMap::new();
        for definition in definitions {
            let name = definition.name.clone();
            if catalog.insert(name, definition).is_some() {
                return Err(PreparationError::new(
                    PreparationErrorCode::InvalidToolDefinition,
                ));
            }
        }
        Ok(Self {
            definitions: catalog,
        })
    }
    /// Validates one untrusted intent and returns an immutable authority-free call.
    pub fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        let (definition, arguments) = self.validate_intent(intent)?;
        if definition.access_contract.is_some() {
            return Err(PreparationError::new(
                PreparationErrorCode::EffectAccessInvalid,
            ));
        }
        PreparedToolCall::from_v1(intent, definition, arguments)
    }

    /// Prepares one v2 call using the exact frozen trusted resolver revision.
    pub fn prepare_v2(
        &self,
        intent: &ToolIntent,
        resolver: &dyn ToolAccessResolver,
    ) -> Result<PreparedToolCall, PreparationError> {
        let (definition, arguments) = self.validate_intent(intent)?;
        if definition.sandbox_requirements.is_some() {
            return Err(PreparationError::new(
                PreparationErrorCode::SandboxRequirementInvalid,
            ));
        }
        let contract = definition
            .access_contract
            .as_ref()
            .ok_or_else(|| PreparationError::new(PreparationErrorCode::EffectAccessInvalid))?;
        if resolver.revision() != contract.resolver_revision {
            return Err(PreparationError::new(
                PreparationErrorCode::EffectAccessInvalid,
            ));
        }
        let accesses = resolver.resolve(&arguments)?;
        let mutating = accesses
            .values()
            .iter()
            .any(|access| access.mode() != AccessMode::Read);
        let requires_mutation = definition.requirements.capabilities().any(|capability| {
            matches!(
                capability,
                ExecutionCapability::FilesystemWrite
                    | ExecutionCapability::Process
                    | ExecutionCapability::BrowserAct
                    | ExecutionCapability::ComputerAct
            )
        });
        if !contract.policy.covers(&accesses)
            || (definition.replay_class == ReplayClass::ReadOnly && mutating)
            || (definition.replay_class != ReplayClass::ReadOnly && requires_mutation && !mutating)
        {
            return Err(PreparationError::new(
                PreparationErrorCode::EffectAccessInvalid,
            ));
        }
        PreparedToolCall::from_v2(intent, definition, arguments, accesses, contract)
    }

    /// Prepares one v3 call bound to exact resources and F0 requirements.
    pub fn prepare_v3(
        &self,
        intent: &ToolIntent,
        resolver: &dyn ToolAccessResolver,
    ) -> Result<PreparedToolCall, PreparationError> {
        let (definition, arguments) = self.validate_intent(intent)?;
        let contract = definition
            .access_contract
            .as_ref()
            .ok_or_else(|| PreparationError::new(PreparationErrorCode::EffectAccessInvalid))?;
        let sandbox = definition.sandbox_requirements.as_ref().ok_or_else(|| {
            PreparationError::new(PreparationErrorCode::SandboxRequirementInvalid)
        })?;
        if resolver.revision() != contract.resolver_revision {
            return Err(PreparationError::new(
                PreparationErrorCode::EffectAccessInvalid,
            ));
        }
        let accesses = resolver.resolve(&arguments)?;
        let mutating = accesses
            .values()
            .iter()
            .any(|access| access.mode() != AccessMode::Read);
        let requires_mutation = definition.requirements.capabilities().any(|capability| {
            matches!(
                capability,
                ExecutionCapability::FilesystemWrite
                    | ExecutionCapability::Process
                    | ExecutionCapability::BrowserAct
                    | ExecutionCapability::ComputerAct
            )
        });
        if !contract.policy.covers(&accesses)
            || (definition.replay_class == ReplayClass::ReadOnly && mutating)
            || (definition.replay_class != ReplayClass::ReadOnly && requires_mutation && !mutating)
        {
            return Err(PreparationError::new(
                PreparationErrorCode::EffectAccessInvalid,
            ));
        }
        PreparedToolCall::from_v3(intent, definition, arguments, accesses, contract, sandbox)
    }

    fn validate_intent<'a>(
        &'a self,
        intent: &ToolIntent,
    ) -> Result<(&'a ToolDefinition, Value), PreparationError> {
        if intent.model_call_id.is_empty() {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidModelCallId,
            ));
        }
        if intent.tool_name.is_empty() {
            return Err(PreparationError::new(PreparationErrorCode::InvalidToolName));
        }
        let definition = self
            .definitions
            .get(&intent.tool_name)
            .ok_or_else(|| PreparationError::new(PreparationErrorCode::ToolNotAdmitted))?;
        let arguments = parse_arguments(&intent.arguments_json)?;
        let failures = validate_arguments(&definition.input_schema, &arguments);
        if !failures.is_empty() {
            return Err(PreparationError::with_failures(
                PreparationErrorCode::ArgumentsSchemaMismatch,
                failures,
            ));
        }
        Ok((definition, arguments))
    }
}

/// Immutable validated call carrying no invocation identity or authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedToolCall {
    model_call_id: String,
    tool_name: String,
    tool_revision: String,
    normalized_arguments: String,
    input_digest: String,
    requirements: ExecutionRequirements,
    replay_class: ReplayClass,
    contract_version: u16,
    access_policy_revision: Option<String>,
    access_resolver_revision: Option<String>,
    invocation_accesses: Option<InvocationAccessSet>,
    max_result_bytes: Option<u64>,
    sandbox_requirements: Option<SandboxRequirementsV1>,
    sandbox_requirements_digest: Option<String>,
}

impl PreparedToolCall {
    fn from_v1(
        intent: &ToolIntent,
        definition: &ToolDefinition,
        arguments: Value,
    ) -> Result<Self, PreparationError> {
        let normalized_arguments = serde_jcs::to_string(&arguments)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        let preimage = json!({ "contract": "garive.prepared-tool-call", "version": 1, "tool_name": definition.name, "tool_revision": definition.revision, "arguments": arguments, "requirements": definition.requirements, "replay_class": definition.replay_class });
        let canonical = serde_jcs::to_vec(&preimage)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        let input_digest = format!("{:x}", Sha256::digest(canonical));
        Ok(Self {
            model_call_id: intent.model_call_id.clone(),
            tool_name: definition.name.clone(),
            tool_revision: definition.revision.clone(),
            normalized_arguments,
            input_digest,
            requirements: definition.requirements.clone(),
            replay_class: definition.replay_class,
            contract_version: 1,
            access_policy_revision: None,
            access_resolver_revision: None,
            invocation_accesses: None,
            max_result_bytes: None,
            sandbox_requirements: None,
            sandbox_requirements_digest: None,
        })
    }

    fn from_v2(
        intent: &ToolIntent,
        definition: &ToolDefinition,
        arguments: Value,
        accesses: InvocationAccessSet,
        contract: &ToolAccessContract,
    ) -> Result<Self, PreparationError> {
        let normalized_arguments = serde_jcs::to_string(&arguments)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        let preimage = json!({
            "contract": "garive.prepared-tool-call",
            "version": 2,
            "tool_name": definition.name,
            "tool_revision": definition.revision,
            "arguments": arguments,
            "requirements": definition.requirements,
            "replay_class": definition.replay_class,
            "access_policy_revision": contract.policy.policy_revision(),
            "access_resolver_revision": contract.resolver_revision,
            "invocation_accesses": accesses,
            "max_result_bytes": contract.policy.max_result_bytes(),
        });
        let canonical = serde_jcs::to_vec(&preimage)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        Ok(Self {
            model_call_id: intent.model_call_id.clone(),
            tool_name: definition.name.clone(),
            tool_revision: definition.revision.clone(),
            normalized_arguments,
            input_digest: format!("{:x}", Sha256::digest(canonical)),
            requirements: definition.requirements.clone(),
            replay_class: definition.replay_class,
            contract_version: 2,
            access_policy_revision: Some(contract.policy.policy_revision().to_owned()),
            access_resolver_revision: Some(contract.resolver_revision.clone()),
            invocation_accesses: Some(accesses),
            max_result_bytes: Some(contract.policy.max_result_bytes()),
            sandbox_requirements: None,
            sandbox_requirements_digest: None,
        })
    }

    fn from_v3(
        intent: &ToolIntent,
        definition: &ToolDefinition,
        arguments: Value,
        accesses: InvocationAccessSet,
        contract: &ToolAccessContract,
        sandbox: &SandboxRequirementsV1,
    ) -> Result<Self, PreparationError> {
        let normalized_arguments = serde_jcs::to_string(&arguments)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        let sandbox_digest = sandbox.digest()?;
        let preimage = json!({
            "contract": "garive.prepared-tool-call",
            "version": 3,
            "tool_name": definition.name,
            "tool_revision": definition.revision,
            "arguments": arguments,
            "requirements": definition.requirements,
            "replay_class": definition.replay_class,
            "access_policy_revision": contract.policy.policy_revision(),
            "access_resolver_revision": contract.resolver_revision,
            "invocation_accesses": accesses,
            "max_result_bytes": contract.policy.max_result_bytes(),
            "sandbox_requirements": sandbox,
            "sandbox_requirements_digest": sandbox_digest,
        });
        let canonical = serde_jcs::to_vec(&preimage)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        Ok(Self {
            model_call_id: intent.model_call_id.clone(),
            tool_name: definition.name.clone(),
            tool_revision: definition.revision.clone(),
            normalized_arguments,
            input_digest: format!("{:x}", Sha256::digest(canonical)),
            requirements: definition.requirements.clone(),
            replay_class: definition.replay_class,
            contract_version: 3,
            access_policy_revision: Some(contract.policy.policy_revision().to_owned()),
            access_resolver_revision: Some(contract.resolver_revision.clone()),
            invocation_accesses: Some(accesses),
            max_result_bytes: Some(contract.policy.max_result_bytes()),
            sandbox_requirements: Some(sandbox.clone()),
            sandbox_requirements_digest: Some(sandbox_digest),
        })
    }
    /// Returns untrusted model correlation retained for observations.
    pub fn model_call_id(&self) -> &str {
        &self.model_call_id
    }
    /// Returns the exact admitted tool name.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }
    /// Returns the exact admitted definition revision.
    pub fn tool_revision(&self) -> &str {
        &self.tool_revision
    }
    /// Returns RFC 8785 canonical argument JSON.
    pub fn normalized_arguments(&self) -> &str {
        &self.normalized_arguments
    }
    /// Returns the lowercase SHA-256 executable-input digest.
    pub fn input_digest(&self) -> &str {
        &self.input_digest
    }
    /// Returns immutable execution requirements.
    pub const fn requirements(&self) -> &ExecutionRequirements {
        &self.requirements
    }
    /// Returns the untrusted recovery declaration.
    pub const fn replay_class(&self) -> ReplayClass {
        self.replay_class
    }

    /// Returns the immutable Prepared Call contract version.
    pub const fn contract_version(&self) -> u16 {
        self.contract_version
    }

    /// Returns the v2 access policy revision, absent for v1.
    pub fn access_policy_revision(&self) -> Option<&str> {
        self.access_policy_revision.as_deref()
    }

    /// Returns the v2 trusted resolver revision, absent for v1.
    pub fn access_resolver_revision(&self) -> Option<&str> {
        self.access_resolver_revision.as_deref()
    }

    /// Returns the v2 exact canonical access set, absent for v1.
    pub const fn invocation_accesses(&self) -> Option<&InvocationAccessSet> {
        self.invocation_accesses.as_ref()
    }

    /// Returns the v2 buffered result charge, absent for v1.
    pub const fn max_result_bytes(&self) -> Option<u64> {
        self.max_result_bytes
    }

    /// Returns the v3 F0 enforcement profile, absent for earlier contracts.
    pub const fn sandbox_requirements(&self) -> Option<&SandboxRequirementsV1> {
        self.sandbox_requirements.as_ref()
    }

    /// Returns the v3 canonical F0 profile digest, absent for earlier contracts.
    pub fn sandbox_requirements_digest(&self) -> Option<&str> {
        self.sandbox_requirements_digest.as_deref()
    }
}
