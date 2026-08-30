//! Typed admitted CDP client operations for managed Browser observation.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use crate::{CdpProtocolError, CdpTransport, CdpTransportError};

/// Browser protocol/build evidence returned by `Browser.getVersion`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpBrowserVersion {
    /// Browser-reported CDP protocol version.
    pub protocol_version: String,
    /// Browser product and version.
    pub product: String,
    /// Browser source revision.
    pub revision: String,
    /// Browser-reported JavaScript engine version.
    pub js_version: String,
}

/// One bounded raw Accessibility property.
#[derive(Clone, Debug, PartialEq)]
pub struct CdpAxProperty {
    /// Official CDP property name.
    pub name: String,
    /// Official typed property value object.
    pub value: Value,
}

/// One bounded raw Accessibility node; IDs remain adapter-private.
#[derive(Clone, Debug, PartialEq)]
pub struct CdpAxNode {
    /// CDP AX node identity.
    pub node_id: String,
    /// Whether Chromium excludes the node from its accessible tree.
    pub ignored: bool,
    /// Computed accessible role.
    pub role: Option<String>,
    /// Computed accessible name.
    pub name: Option<String>,
    /// Bounded value summary before Runtime sensitivity classification.
    pub value_summary: Option<String>,
    /// Raw bounded state/property values.
    pub properties: Vec<CdpAxProperty>,
    /// Optional CDP parent identity.
    pub parent_id: Option<String>,
    /// Ordered CDP child identities.
    pub child_ids: Vec<String>,
    /// Optional associated backend DOM node identity.
    pub backend_dom_node_id: Option<u64>,
    /// Optional owning frame identity.
    pub frame_id: Option<String>,
}

/// One complete bounded raw Accessibility tree.
#[derive(Clone, Debug, PartialEq)]
pub struct CdpAxTree {
    /// Nodes in browser-returned order.
    pub nodes: Vec<CdpAxNode>,
}

/// Sequential typed client over one exact managed-browser transport.
pub struct CdpClient {
    transport: CdpTransport,
}

impl CdpClient {
    /// Wraps an already connected bounded transport.
    pub fn new(transport: CdpTransport) -> Self {
        Self { transport }
    }

    /// Records exact protocol/build evidence before target operations.
    pub async fn browser_version(&mut self) -> Result<CdpBrowserVersion, CdpTransportError> {
        let result = self
            .transport
            .call("Browser.getVersion", json!({}), None)
            .await?;
        Ok(CdpBrowserVersion {
            protocol_version: bounded_text(&result, "protocolVersion", 128)?,
            product: bounded_text(&result, "product", 512)?,
            revision: bounded_text(&result, "revision", 512)?,
            js_version: bounded_text(&result, "jsVersion", 128)?,
        })
    }

    /// Attaches to one already-admitted page target using flat session routing.
    pub async fn attach_target(&mut self, target_id: &str) -> Result<String, CdpTransportError> {
        validate_id(target_id)?;
        let result = self
            .transport
            .call(
                "Target.attachToTarget",
                json!({"targetId":target_id,"flatten":true}),
                None,
            )
            .await?;
        bounded_text(&result, "sessionId", 4_096)
    }

    /// Enables stable AX node identities for one flat target session.
    pub async fn enable_accessibility(
        &mut self,
        session_id: &str,
    ) -> Result<(), CdpTransportError> {
        validate_id(session_id)?;
        self.transport
            .call("Accessibility.enable", json!({}), Some(session_id.into()))
            .await?;
        Ok(())
    }

    /// Fetches and validates one full AX tree under explicit depth/node/text bounds.
    pub async fn full_ax_tree(
        &mut self,
        session_id: &str,
        frame_id: Option<&str>,
        depth: u32,
        max_nodes: usize,
        max_text_bytes: usize,
    ) -> Result<CdpAxTree, CdpTransportError> {
        validate_id(session_id)?;
        if depth == 0
            || depth > 128
            || max_nodes == 0
            || max_nodes > 10_000
            || max_text_bytes == 0
            || max_text_bytes > 1_048_576
        {
            return Err(protocol());
        }
        let mut params = Map::from_iter([("depth".into(), Value::from(depth))]);
        if let Some(frame_id) = frame_id {
            validate_id(frame_id)?;
            params.insert("frameId".into(), Value::String(frame_id.into()));
        }
        let result = self
            .transport
            .call(
                "Accessibility.getFullAXTree",
                Value::Object(params),
                Some(session_id.into()),
            )
            .await?;
        parse_tree(&result, max_nodes, max_text_bytes)
    }
}

fn parse_tree(
    result: &Value,
    max_nodes: usize,
    max_text_bytes: usize,
) -> Result<CdpAxTree, CdpTransportError> {
    let values = result
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(protocol)?;
    if values.len() > max_nodes {
        return Err(protocol());
    }
    let mut nodes = Vec::with_capacity(values.len());
    let mut identities = BTreeSet::new();
    let mut text_bytes = 0usize;
    for value in values {
        let object = value.as_object().ok_or_else(protocol)?;
        let node_id = object_text(object, "nodeId", 4_096)?;
        if !identities.insert(node_id.clone()) {
            return Err(protocol());
        }
        let role = ax_text(object.get("role"))?;
        let name = ax_text(object.get("name"))?;
        let value_summary = ax_text(object.get("value"))?;
        let properties = object
            .get("properties")
            .map(parse_properties)
            .transpose()?
            .unwrap_or_default();
        text_bytes = text_bytes
            .checked_add(node_id.len())
            .and_then(|count| count.checked_add(optional_len(&role)))
            .and_then(|count| count.checked_add(optional_len(&name)))
            .and_then(|count| count.checked_add(optional_len(&value_summary)))
            .and_then(|count| {
                properties.iter().try_fold(count, |count, property| {
                    count
                        .checked_add(property.name.len())?
                        .checked_add(property.value.to_string().len())
                })
            })
            .ok_or_else(protocol)?;
        if text_bytes > max_text_bytes {
            return Err(protocol());
        }
        nodes.push(CdpAxNode {
            node_id,
            ignored: object
                .get("ignored")
                .and_then(Value::as_bool)
                .ok_or_else(protocol)?,
            role,
            name,
            value_summary,
            properties,
            parent_id: optional_object_text(object, "parentId", 4_096)?,
            child_ids: optional_text_array(object, "childIds", 4_096)?,
            backend_dom_node_id: object
                .get("backendDOMNodeId")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|value| *value > 0)
                        .ok_or_else(protocol)
                })
                .transpose()?,
            frame_id: optional_object_text(object, "frameId", 4_096)?,
        });
    }
    Ok(CdpAxTree { nodes })
}

fn parse_properties(value: &Value) -> Result<Vec<CdpAxProperty>, CdpTransportError> {
    let values = value.as_array().ok_or_else(protocol)?;
    if values.len() > 128 {
        return Err(protocol());
    }
    values
        .iter()
        .map(|value| {
            let object = value.as_object().ok_or_else(protocol)?;
            Ok(CdpAxProperty {
                name: object_text(object, "name", 128)?,
                value: object.get("value").cloned().ok_or_else(protocol)?,
            })
        })
        .collect()
}

fn ax_text(value: Option<&Value>) -> Result<Option<String>, CdpTransportError> {
    value
        .map(|value| {
            let object = value.as_object().ok_or_else(protocol)?;
            match object.get("value") {
                Some(Value::String(value)) if value.len() <= 32_768 => Ok(Some(value.clone())),
                Some(Value::Null) | None => Ok(None),
                Some(value) if value.is_boolean() || value.is_number() => {
                    Ok(Some(value.to_string()))
                }
                _ => Err(protocol()),
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn bounded_text(value: &Value, field: &str, max: usize) -> Result<String, CdpTransportError> {
    value
        .as_object()
        .ok_or_else(protocol)
        .and_then(|object| object_text(object, field, max))
}

fn object_text(
    object: &Map<String, Value>,
    field: &str,
    max: usize,
) -> Result<String, CdpTransportError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= max)
        .map(str::to_owned)
        .ok_or_else(protocol)
}

fn optional_object_text(
    object: &Map<String, Value>,
    field: &str,
    max: usize,
) -> Result<Option<String>, CdpTransportError> {
    object
        .get(field)
        .map(|_| object_text(object, field, max))
        .transpose()
}

fn optional_text_array(
    object: &Map<String, Value>,
    field: &str,
    max: usize,
) -> Result<Vec<String>, CdpTransportError> {
    object
        .get(field)
        .map(|value| {
            value
                .as_array()
                .ok_or_else(protocol)?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .filter(|value| !value.is_empty() && value.len() <= max)
                        .map(str::to_owned)
                        .ok_or_else(protocol)
                })
                .collect()
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn validate_id(value: &str) -> Result<(), CdpTransportError> {
    if value.is_empty() || value.len() > 4_096 {
        Err(protocol())
    } else {
        Ok(())
    }
}

fn optional_len(value: &Option<String>) -> usize {
    value.as_ref().map_or(0, String::len)
}

fn protocol() -> CdpTransportError {
    CdpTransportError::Protocol(CdpProtocolError::InvalidMessage)
}
