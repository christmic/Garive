//! Exact T2 browser definitions and pure session/origin bindings.

use serde_json::{json, Value};

use crate::{
    AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionCapability, ExecutionRequirements,
    InvocationAccessSet, PreparationError, PreparationErrorCode, PreparedToolCall, ReplayClass,
    ResourceAccess, SandboxControl, SandboxRequirementsV1, ToolAccessPolicyV1, ToolAccessResolver,
    ToolCatalog, ToolDefinition, ToolIntent,
};

/// Exact T2 Browser tool revision.
pub const T2_BROWSER_TOOL_REVISION: &str = "1";
/// Pure T2 Browser binding implementation revision.
pub const T2_BROWSER_RESOLVER_REVISION: &str = "garive.t2.browser.access.v1";
/// Observe one admitted browser page.
pub const T2_BROWSER_OBSERVE: &str = "garive.browser.observe";
/// Navigate one admitted browser page.
pub const T2_BROWSER_NAVIGATE: &str = "garive.browser.navigate";
/// Perform one bounded semantic browser action.
pub const T2_BROWSER_ACT: &str = "garive.browser.act";

const MAX_RESULT_BYTES: u64 = 2_097_152;

/// Validated exact browser session/page identity admitted by a catalogue.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BrowserPageScope {
    session_id: String,
    page_id: String,
}

impl BrowserPageScope {
    /// Constructs one scope from portable non-empty ASCII identifiers.
    pub fn new(
        session_id: impl Into<String>,
        page_id: impl Into<String>,
    ) -> Result<Self, PreparationError> {
        let value = Self {
            session_id: session_id.into(),
            page_id: page_id.into(),
        };
        if !token(&value.session_id) || !token(&value.page_id) {
            return Err(access_error());
        }
        Ok(value)
    }

    fn resource_key(&self) -> String {
        format!("browser:{}:{}", self.session_id, self.page_id)
    }
}

/// Frozen three-tool Browser catalogue for one exact capability snapshot.
#[derive(Clone, Debug)]
pub struct BuiltinT2BrowserCatalogue {
    definitions: Vec<ToolDefinition>,
    catalog: ToolCatalog,
}

impl BuiltinT2BrowserCatalogue {
    /// Freezes exact admitted pages, canonical origins and policy revision.
    pub fn new(
        policy_revision: impl Into<String>,
        pages: impl IntoIterator<Item = BrowserPageScope>,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, PreparationError> {
        let policy_revision = policy_revision.into();
        let pages = pages.into_iter().collect::<Vec<_>>();
        let origins = origins.into_iter().map(Into::into).collect::<Vec<_>>();
        let mut definitions = vec![
            observe_definition(&policy_revision, &pages)?,
            navigate_definition(&policy_revision, &pages, &origins)?,
            act_definition(&policy_revision, &pages, &origins)?,
        ];
        definitions.sort_by(|left, right| left.name().cmp(right.name()));
        let catalog = ToolCatalog::new(definitions.clone())?;
        Ok(Self {
            definitions,
            catalog,
        })
    }

    /// Returns the exact definitions frozen into the Agent snapshot.
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    /// Prepares one Browser intent through exact page and origin bindings.
    pub fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.catalog.prepare_v3(
            intent,
            &BrowserResolver {
                tool_name: intent.tool_name(),
            },
        )
    }
}

struct BrowserResolver<'a> {
    tool_name: &'a str,
}

impl ToolAccessResolver for BrowserResolver<'_> {
    fn revision(&self) -> &str {
        T2_BROWSER_RESOLVER_REVISION
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        let page =
            BrowserPageScope::new(text(arguments, "session_id")?, text(arguments, "page_id")?)?;
        let mode = if self.tool_name == T2_BROWSER_OBSERVE {
            AccessMode::Read
        } else {
            AccessMode::Write
        };
        let mut accesses = vec![ResourceAccess::new(
            AccessNamespace::Runtime,
            page.resource_key(),
            mode,
        )?];
        match self.tool_name {
            T2_BROWSER_OBSERVE => {}
            T2_BROWSER_NAVIGATE => {
                let origin = text(arguments, "destination_origin")?;
                if url_origin(text(arguments, "destination_url")?) != Some(origin) {
                    return Err(access_error());
                }
                accesses.push(ResourceAccess::new(
                    AccessNamespace::Network,
                    origin,
                    AccessMode::Write,
                )?);
            }
            T2_BROWSER_ACT => {
                validate_action(arguments)?;
                for origin in string_array(arguments, "allowed_navigation_origins")? {
                    accesses.push(ResourceAccess::new(
                        AccessNamespace::Network,
                        origin,
                        AccessMode::Write,
                    )?);
                }
            }
            _ => return Err(access_error()),
        }
        InvocationAccessSet::new(accesses)
    }
}

fn observe_definition(
    policy: &str,
    pages: &[BrowserPageScope],
) -> Result<ToolDefinition, PreparationError> {
    definition(
        T2_BROWSER_OBSERVE,
        "Observe one bounded browser semantic tree.",
        json!({"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"page_id":{"type":"string","minLength":1,"maxLength":128},"expected_previous_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"max_nodes":{"type":"integer","minimum":1,"maximum":10000},"max_text_bytes":{"type":"integer","minimum":1,"maximum":1048576}},"required":["session_id","page_id","max_nodes","max_text_bytes"],"additionalProperties":false}),
        [ExecutionCapability::BrowserObserve],
        ReplayClass::ReadOnly,
        pages,
        &[],
        AccessMode::Read,
        policy,
    )
}

fn navigate_definition(
    policy: &str,
    pages: &[BrowserPageScope],
    origins: &[String],
) -> Result<ToolDefinition, PreparationError> {
    definition(
        T2_BROWSER_NAVIGATE,
        "Navigate one browser page to an exact admitted origin.",
        json!({"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"page_id":{"type":"string","minLength":1,"maxLength":128},"expected_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"target_revision":{"type":"string","minLength":1,"maxLength":128},"destination_url":{"type":"string","minLength":1,"maxLength":8192},"destination_origin":{"type":"string","minLength":10,"maxLength":512},"wait_until":{"type":"string","enum":["dom_content_loaded","load","network_idle"]},"timeout_ms":{"type":"integer","minimum":1,"maximum":120000},"max_nodes":{"type":"integer","minimum":1,"maximum":10000},"max_text_bytes":{"type":"integer","minimum":1,"maximum":1048576}},"required":["session_id","page_id","expected_snapshot_id","target_revision","destination_url","destination_origin","wait_until","timeout_ms","max_nodes","max_text_bytes"],"additionalProperties":false}),
        [
            ExecutionCapability::BrowserAct,
            ExecutionCapability::Network,
        ],
        ReplayClass::NeverReplay,
        pages,
        origins,
        AccessMode::Write,
        policy,
    )
}

fn act_definition(
    policy: &str,
    pages: &[BrowserPageScope],
    origins: &[String],
) -> Result<ToolDefinition, PreparationError> {
    definition(
        T2_BROWSER_ACT,
        "Perform one snapshot-bound semantic browser action.",
        json!({"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"page_id":{"type":"string","minLength":1,"maxLength":128},"expected_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"target_revision":{"type":"string","minLength":1,"maxLength":128},"action":{"type":"string","enum":["click","type_text","clear","select_option","press_key","scroll","go_back","go_forward","reload"]},"node_ref":{"type":"string","minLength":1,"maxLength":128},"text":{"type":"string","maxLength":32768},"option":{"type":"string","minLength":1,"maxLength":4096},"key":{"type":"string","enum":["enter","tab","escape","backspace","delete","arrow_up","arrow_down","arrow_left","arrow_right","home","end","page_up","page_down","space"]},"delta_x":{"type":"integer","minimum":-100000,"maximum":100000},"delta_y":{"type":"integer","minimum":-100000,"maximum":100000},"allowed_navigation_origins":{"type":"array","maxItems":16,"items":{"type":"string","minLength":10,"maxLength":512}}},"required":["session_id","page_id","expected_snapshot_id","target_revision","action","allowed_navigation_origins"],"additionalProperties":false}),
        [
            ExecutionCapability::BrowserAct,
            ExecutionCapability::Network,
        ],
        ReplayClass::NeverReplay,
        pages,
        origins,
        AccessMode::Write,
        policy,
    )
}

#[allow(clippy::too_many_arguments)]
fn definition<const C: usize>(
    name: &str,
    description: &str,
    schema: Value,
    capabilities: [ExecutionCapability; C],
    replay: ReplayClass,
    pages: &[BrowserPageScope],
    origins: &[String],
    page_mode: AccessMode,
    policy: &str,
) -> Result<ToolDefinition, PreparationError> {
    let observe = name == T2_BROWSER_OBSERVE;
    let requirements = ExecutionRequirements::new(
        capabilities,
        if observe { 30_000 } else { 120_000 },
        MAX_RESULT_BYTES,
    )?;
    let controls = if capabilities.contains(&ExecutionCapability::Network) {
        vec![
            SandboxControl::NetworkOriginScope,
            SandboxControl::RedirectRevalidation,
            SandboxControl::BrowserSessionScope,
            SandboxControl::SnapshotBinding,
            SandboxControl::ResourceLimits,
        ]
    } else {
        vec![
            SandboxControl::BrowserSessionScope,
            SandboxControl::SnapshotBinding,
            SandboxControl::ResourceLimits,
        ]
    };
    ToolDefinition::new_v3(
        name,
        T2_BROWSER_TOOL_REVISION,
        description,
        schema,
        requirements.clone(),
        replay,
        ToolAccessPolicyV1::new(
            policy,
            [],
            [],
            origins
                .iter()
                .map(|origin| AccessPolicyEntry::new(origin, [AccessMode::Write]))
                .collect::<Result<Vec<_>, _>>()?,
            pages
                .iter()
                .map(|page| AccessPolicyEntry::new(page.resource_key(), [page_mode]))
                .collect::<Result<Vec<_>, _>>()?,
            if observe { 1 } else { 17 },
            MAX_RESULT_BYTES,
        )?,
        T2_BROWSER_RESOLVER_REVISION,
        SandboxRequirementsV1::new(requirements.capabilities(), controls, None, 64)?,
    )
}

fn validate_action(arguments: &Value) -> Result<(), PreparationError> {
    let present = |name: &str| arguments.get(name).is_some();
    let valid = match text(arguments, "action")? {
        "click" | "clear" => {
            present("node_ref")
                && !present("text")
                && !present("option")
                && !present("key")
                && !present("delta_x")
                && !present("delta_y")
        }
        "type_text" => {
            present("node_ref")
                && present("text")
                && !present("option")
                && !present("key")
                && !present("delta_x")
                && !present("delta_y")
        }
        "select_option" => {
            present("node_ref")
                && present("option")
                && !present("text")
                && !present("key")
                && !present("delta_x")
                && !present("delta_y")
        }
        "press_key" => {
            present("key")
                && !present("node_ref")
                && !present("text")
                && !present("option")
                && !present("delta_x")
                && !present("delta_y")
        }
        "scroll" => {
            present("delta_x")
                && present("delta_y")
                && (integer(arguments, "delta_x")? != 0 || integer(arguments, "delta_y")? != 0)
                && !present("node_ref")
                && !present("text")
                && !present("option")
                && !present("key")
        }
        "go_back" | "go_forward" | "reload" => {
            !["node_ref", "text", "option", "key", "delta_x", "delta_y"]
                .iter()
                .any(|name| present(name))
        }
        _ => false,
    };
    if !valid {
        return Err(access_error());
    }
    Ok(())
}

fn string_array<'a>(value: &'a Value, field: &str) -> Result<Vec<&'a str>, PreparationError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(access_error)?
        .iter()
        .map(|value| value.as_str().ok_or_else(access_error))
        .collect()
}

fn integer(value: &Value, field: &str) -> Result<i64, PreparationError> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(access_error)
}

fn url_origin(url: &str) -> Option<&str> {
    let scheme_end = url.find("://")?;
    if !matches!(&url[..scheme_end], "http" | "https") {
        return None;
    }
    let authority_end = url[scheme_end + 3..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |index| scheme_end + 3 + index);
    Some(&url[..authority_end])
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, PreparationError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(access_error)
}

fn access_error() -> PreparationError {
    PreparationError::new(PreparationErrorCode::EffectAccessInvalid)
}
