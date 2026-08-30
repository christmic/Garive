//! Exact T2 Computer Use definitions and pure target/action bindings.

use serde_json::{json, Map, Value};

use crate::{
    AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionCapability, ExecutionRequirements,
    InvocationAccessSet, PreparationError, PreparationErrorCode, PreparedToolCall, ReplayClass,
    ResourceAccess, SandboxControl, SandboxRequirementsV1, ToolAccessPolicyV1, ToolAccessResolver,
    ToolCatalog, ToolDefinition, ToolIntent,
};

/// Exact T2 Computer Use tool revision.
pub const T2_COMPUTER_TOOL_REVISION: &str = "1";
/// Pure T2 Computer Use binding implementation revision.
pub const T2_COMPUTER_RESOLVER_REVISION: &str = "garive.t2.computer.access.v1";
/// Observe one admitted native application window.
pub const T2_COMPUTER_OBSERVE: &str = "garive.computer.observe";
/// Perform one snapshot-bound native action.
pub const T2_COMPUTER_ACT: &str = "garive.computer.act";

const MAX_RESULT_BYTES: u64 = 2_097_152;

/// Runtime-owned opaque identity for one admitted native application window.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ComputerTargetScope {
    desktop_session_id: String,
    application_id: String,
    window_id: String,
}

impl ComputerTargetScope {
    /// Constructs one exact target from portable opaque Runtime identifiers.
    pub fn new(
        desktop_session_id: impl Into<String>,
        application_id: impl Into<String>,
        window_id: impl Into<String>,
    ) -> Result<Self, PreparationError> {
        let value = Self {
            desktop_session_id: desktop_session_id.into(),
            application_id: application_id.into(),
            window_id: window_id.into(),
        };
        if !token(&value.desktop_session_id)
            || !token(&value.application_id)
            || !token(&value.window_id)
        {
            return Err(access_error());
        }
        Ok(value)
    }

    fn resource_key(&self) -> String {
        format!(
            "computer:{}:{}:{}",
            self.desktop_session_id, self.application_id, self.window_id
        )
    }
}

/// Frozen Computer observe/act catalogue for one capability snapshot.
#[derive(Clone, Debug)]
pub struct BuiltinT2ComputerCatalogue {
    definitions: Vec<ToolDefinition>,
    catalog: ToolCatalog,
}

impl BuiltinT2ComputerCatalogue {
    /// Freezes an explicit policy revision and exact target identities.
    pub fn new(
        policy_revision: impl Into<String>,
        targets: impl IntoIterator<Item = ComputerTargetScope>,
    ) -> Result<Self, PreparationError> {
        let policy_revision = policy_revision.into();
        let targets = targets.into_iter().collect::<Vec<_>>();
        let mut definitions = vec![
            observe_definition(&policy_revision, &targets)?,
            act_definition(&policy_revision, &targets)?,
        ];
        definitions.sort_by(|left, right| left.name().cmp(right.name()));
        let catalog = ToolCatalog::new(definitions.clone())?;
        Ok(Self {
            definitions,
            catalog,
        })
    }

    /// Returns exact definitions frozen into the Agent snapshot.
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    /// Prepares one Computer intent through exact target/action bindings.
    pub fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.catalog.prepare_v3(
            intent,
            &ComputerResolver {
                tool_name: intent.tool_name(),
            },
        )
    }
}

struct ComputerResolver<'a> {
    tool_name: &'a str,
}

impl ToolAccessResolver for ComputerResolver<'_> {
    fn revision(&self) -> &str {
        T2_COMPUTER_RESOLVER_REVISION
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        let target = ComputerTargetScope::new(
            text(arguments, "desktop_session_id")?,
            text(arguments, "application_id")?,
            text(arguments, "window_id")?,
        )?;
        let mode = match self.tool_name {
            T2_COMPUTER_OBSERVE => AccessMode::Read,
            T2_COMPUTER_ACT => {
                validate_action(arguments)?;
                AccessMode::Write
            }
            _ => return Err(access_error()),
        };
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Runtime,
            target.resource_key(),
            mode,
        )?])
    }
}

fn observe_definition(
    policy: &str,
    targets: &[ComputerTargetScope],
) -> Result<ToolDefinition, PreparationError> {
    definition(
        T2_COMPUTER_OBSERVE,
        "Observe one bounded native accessibility tree and optional window capture.",
        json!({"type":"object","properties":{"desktop_session_id":{"type":"string","minLength":1,"maxLength":128},"application_id":{"type":"string","minLength":1,"maxLength":128},"window_id":{"type":"string","minLength":1,"maxLength":128},"expected_previous_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"max_nodes":{"type":"integer","minimum":1,"maximum":10000},"max_text_bytes":{"type":"integer","minimum":1,"maximum":1048576},"capture":{"type":"string","enum":["none","window"]},"max_capture_bytes":{"type":"integer","minimum":1,"maximum":8388608},"max_capture_pixels":{"type":"integer","minimum":1,"maximum":16777216}},"required":["desktop_session_id","application_id","window_id","max_nodes","max_text_bytes","capture","max_capture_bytes","max_capture_pixels"],"additionalProperties":false}),
        ExecutionCapability::ComputerObserve,
        ReplayClass::ReadOnly,
        AccessMode::Read,
        policy,
        targets,
    )
}

fn act_definition(
    policy: &str,
    targets: &[ComputerTargetScope],
) -> Result<ToolDefinition, PreparationError> {
    definition(
        T2_COMPUTER_ACT,
        "Perform one exact snapshot-bound native semantic or coordinate action.",
        json!({"type":"object","properties":{"desktop_session_id":{"type":"string","minLength":1,"maxLength":128},"application_id":{"type":"string","minLength":1,"maxLength":128},"window_id":{"type":"string","minLength":1,"maxLength":128},"expected_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"target_revision":{"type":"string","minLength":1,"maxLength":128},"action":{"type":"string","enum":["press","set_value","type_text","press_key","scroll","move_pointer","click_point","drag"]},"node_ref":{"type":"string","minLength":1,"maxLength":128},"value":{"type":"string","maxLength":32768},"text":{"type":"string","maxLength":32768},"key":{"type":"string","enum":["enter","tab","escape","backspace","delete","arrow_up","arrow_down","arrow_left","arrow_right","home","end","page_up","page_down","space"]},"delta_x":{"type":"integer","minimum":-100000,"maximum":100000},"delta_y":{"type":"integer","minimum":-100000,"maximum":100000},"display_id":{"type":"string","minLength":1,"maxLength":128},"point_x":{"type":"integer","minimum":0,"maximum":1000000},"point_y":{"type":"integer","minimum":0,"maximum":1000000},"start_x":{"type":"integer","minimum":0,"maximum":1000000},"start_y":{"type":"integer","minimum":0,"maximum":1000000},"end_x":{"type":"integer","minimum":0,"maximum":1000000},"end_y":{"type":"integer","minimum":0,"maximum":1000000},"snapshot_pixel_width":{"type":"integer","minimum":1,"maximum":32768},"snapshot_pixel_height":{"type":"integer","minimum":1,"maximum":32768},"scale_milli":{"type":"integer","minimum":1000,"maximum":8000},"visible_frame_x":{"type":"integer","minimum":0,"maximum":1000000},"visible_frame_y":{"type":"integer","minimum":0,"maximum":1000000},"visible_frame_width":{"type":"integer","minimum":1,"maximum":32768},"visible_frame_height":{"type":"integer","minimum":1,"maximum":32768}},"required":["desktop_session_id","application_id","window_id","expected_snapshot_id","target_revision","action"],"additionalProperties":false}),
        ExecutionCapability::ComputerAct,
        ReplayClass::NeverReplay,
        AccessMode::Write,
        policy,
        targets,
    )
}

#[allow(clippy::too_many_arguments)]
fn definition(
    name: &str,
    description: &str,
    schema: Value,
    capability: ExecutionCapability,
    replay: ReplayClass,
    mode: AccessMode,
    policy: &str,
    targets: &[ComputerTargetScope],
) -> Result<ToolDefinition, PreparationError> {
    let requirements = ExecutionRequirements::new([capability], 30_000, MAX_RESULT_BYTES)?;
    let mut controls = vec![
        SandboxControl::NativeTargetScope,
        SandboxControl::SnapshotBinding,
        SandboxControl::ScreenCaptureScope,
        SandboxControl::ResourceLimits,
    ];
    if capability == ExecutionCapability::ComputerAct {
        controls.push(SandboxControl::FocusRevalidation);
    }
    ToolDefinition::new_v3(
        name,
        T2_COMPUTER_TOOL_REVISION,
        description,
        schema,
        requirements.clone(),
        replay,
        ToolAccessPolicyV1::new(
            policy,
            [],
            [],
            [],
            targets
                .iter()
                .map(|target| AccessPolicyEntry::new(target.resource_key(), [mode]))
                .collect::<Result<Vec<_>, _>>()?,
            1,
            MAX_RESULT_BYTES,
        )?,
        T2_COMPUTER_RESOLVER_REVISION,
        SandboxRequirementsV1::new(requirements.capabilities(), controls, None, 64)?,
    )
}

fn validate_action(arguments: &Value) -> Result<(), PreparationError> {
    let object = arguments.as_object().ok_or_else(access_error)?;
    let action = text(arguments, "action")?;
    let valid = match action {
        "press" => present(object, "node_ref") && absent(object, &DETAIL_FIELDS[1..]),
        "set_value" => {
            present(object, "node_ref")
                && present(object, "value")
                && absent(object, &DETAIL_FIELDS[2..])
        }
        "type_text" => {
            present(object, "node_ref")
                && present(object, "text")
                && !present(object, "value")
                && absent(object, &DETAIL_FIELDS[3..])
        }
        "press_key" => {
            present(object, "key")
                && !present(object, "node_ref")
                && absent(object, &["value", "text"])
                && absent(object, &DETAIL_FIELDS[4..])
        }
        "scroll" => semantic_scroll(object)?,
        "move_pointer" | "click_point" => point_action(object)?,
        "drag" => drag_action(object)?,
        _ => false,
    };
    if !valid {
        return Err(access_error());
    }
    Ok(())
}

const DETAIL_FIELDS: [&str; 20] = [
    "node_ref",
    "value",
    "text",
    "key",
    "delta_x",
    "delta_y",
    "display_id",
    "point_x",
    "point_y",
    "start_x",
    "start_y",
    "end_x",
    "end_y",
    "snapshot_pixel_width",
    "snapshot_pixel_height",
    "scale_milli",
    "visible_frame_x",
    "visible_frame_y",
    "visible_frame_width",
    "visible_frame_height",
];
const GEOMETRY_FIELDS: [&str; 8] = [
    "display_id",
    "snapshot_pixel_width",
    "snapshot_pixel_height",
    "scale_milli",
    "visible_frame_x",
    "visible_frame_y",
    "visible_frame_width",
    "visible_frame_height",
];

fn semantic_scroll(object: &Map<String, Value>) -> Result<bool, PreparationError> {
    Ok(present(object, "node_ref")
        && present(object, "delta_x")
        && present(object, "delta_y")
        && (integer(object, "delta_x")? != 0 || integer(object, "delta_y")? != 0)
        && absent(object, &["value", "text", "key"])
        && absent(object, &DETAIL_FIELDS[6..20]))
}

fn point_action(object: &Map<String, Value>) -> Result<bool, PreparationError> {
    Ok(GEOMETRY_FIELDS.iter().all(|field| present(object, field))
        && present(object, "point_x")
        && present(object, "point_y")
        && absent(
            object,
            &["node_ref", "value", "text", "key", "delta_x", "delta_y"],
        )
        && absent(object, &["start_x", "start_y", "end_x", "end_y"])
        && geometry_valid(object)?
        && point_inside(
            object,
            integer(object, "point_x")?,
            integer(object, "point_y")?,
        )?)
}

fn drag_action(object: &Map<String, Value>) -> Result<bool, PreparationError> {
    let coordinates = ["start_x", "start_y", "end_x", "end_y"];
    let (start_x, start_y, end_x, end_y) = (
        integer(object, "start_x")?,
        integer(object, "start_y")?,
        integer(object, "end_x")?,
        integer(object, "end_y")?,
    );
    Ok(GEOMETRY_FIELDS.iter().all(|field| present(object, field))
        && coordinates.iter().all(|field| present(object, field))
        && absent(
            object,
            &["node_ref", "value", "text", "key", "delta_x", "delta_y"],
        )
        && absent(object, &["point_x", "point_y"])
        && (start_x != end_x || start_y != end_y)
        && geometry_valid(object)?
        && point_inside(object, start_x, start_y)?
        && point_inside(object, end_x, end_y)?)
}

fn geometry_valid(object: &Map<String, Value>) -> Result<bool, PreparationError> {
    let width = integer(object, "snapshot_pixel_width")?;
    let height = integer(object, "snapshot_pixel_height")?;
    let x = integer(object, "visible_frame_x")?;
    let y = integer(object, "visible_frame_y")?;
    let frame_width = integer(object, "visible_frame_width")?;
    let frame_height = integer(object, "visible_frame_height")?;
    let display = object
        .get("display_id")
        .and_then(Value::as_str)
        .is_some_and(token);
    Ok(display
        && x.checked_add(frame_width).is_some_and(|end| end <= width)
        && y.checked_add(frame_height).is_some_and(|end| end <= height))
}

fn point_inside(object: &Map<String, Value>, x: i64, y: i64) -> Result<bool, PreparationError> {
    let left = integer(object, "visible_frame_x")?;
    let top = integer(object, "visible_frame_y")?;
    let right = left
        .checked_add(integer(object, "visible_frame_width")?)
        .ok_or_else(access_error)?;
    let bottom = top
        .checked_add(integer(object, "visible_frame_height")?)
        .ok_or_else(access_error)?;
    Ok(x >= left && x < right && y >= top && y < bottom)
}

fn present(object: &Map<String, Value>, field: &str) -> bool {
    object.contains_key(field)
}
fn absent(object: &Map<String, Value>, fields: &[&str]) -> bool {
    fields.iter().all(|field| !present(object, field))
}
fn integer(object: &Map<String, Value>, field: &str) -> Result<i64, PreparationError> {
    object
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(access_error)
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
