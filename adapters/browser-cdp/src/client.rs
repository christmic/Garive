//! Typed admitted CDP client operations for managed Browser observation.

use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use serde_json::{json, Map, Value};
use url::Url;

use crate::{CdpProtocolError, CdpTransport, CdpTransportError};

const MAX_ACTION_TEXT_BYTES: usize = 32_768;
const MAX_OPTION_SCALARS: usize = 4_096;
const MAX_OPTION_UTF8_BYTES: usize = 16_384;
const MAX_FRAME_COUNT: usize = 256;
const MAX_FRAME_DEPTH: usize = 64;
const MAX_FRAME_TEXT_BYTES: usize = 1_048_576;
const MAX_SCROLL_DELTA: u64 = 100_000;
const MAX_LAYOUT_EXTENT: f64 = 10_000_000.0;
const SCROLL_SETTLE_ATTEMPTS: usize = 50;
const SCROLL_SETTLE_INTERVAL: Duration = Duration::from_millis(10);
const SELECT_OPTION_FUNCTION: &str = r#"function(value) {
    if (!(this instanceof HTMLSelectElement)) return {status: "unavailable"};
    const matches = Array.from(this.options).filter(option => option.value === value);
    if (matches.length !== 1) return {status: "unavailable"};
    const option = matches[0];
    if (option.disabled || (option.parentElement instanceof HTMLOptGroupElement && option.parentElement.disabled)) {
        return {status: "unavailable"};
    }
    const before = this.value;
    this.value = value;
    if (this.value !== value) return {status: "unavailable"};
    const changed = before !== this.value;
    if (changed) {
        this.dispatchEvent(new Event("input", {bubbles: true, composed: true}));
        this.dispatchEvent(new Event("change", {bubbles: true}));
    }
    return {status: "selected", changed, value: this.value};
}"#;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CdpViewportMetrics {
    page_x: f64,
    page_y: f64,
    client_width: f64,
    client_height: f64,
    content_width: f64,
    content_height: f64,
}

impl CdpViewportMetrics {
    fn can_move(self, delta_x: i64, delta_y: i64) -> bool {
        let max_x = (self.content_width - self.client_width).max(0.0);
        let max_y = (self.content_height - self.client_height).max(0.0);
        (delta_x > 0 && self.page_x < max_x)
            || (delta_x < 0 && self.page_x > 0.0)
            || (delta_y > 0 && self.page_y < max_y)
            || (delta_y < 0 && self.page_y > 0.0)
    }

    fn position_changed(self, other: Self) -> bool {
        self.page_x != other.page_x || self.page_y != other.page_y
    }
}

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

/// Trustworthy terminal classification from the fixed native select operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdpSelectOutcome {
    /// One exact enabled option was selected; `changed` reports value movement.
    Selected {
        /// Whether the selected value differed before dispatch.
        changed: bool,
    },
    /// The target was not a native select or the option was absent/ambiguous/disabled.
    OptionUnavailable,
}

/// Exact T2 page readiness condition mapped to CDP events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdpWaitUntil {
    /// Main document DOM content loaded event.
    DomContentLoaded,
    /// Main page load event.
    Load,
    /// Main frame CDP lifecycle `networkIdle` event.
    NetworkIdle,
}

/// Closed portable keyboard catalogue independent of platform scan codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CdpPortableKey {
    /// Enter or Return.
    Enter,
    /// Tab focus traversal.
    Tab,
    /// Escape.
    Escape,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Space.
    Space,
}

/// Typed navigation result used by Runtime redirect/origin revalidation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpNavigationResult {
    /// Navigated main frame identity.
    pub frame_id: String,
    /// Optional new-document loader identity.
    pub loader_id: Option<String>,
    /// Final committed frame URL after redirects.
    pub final_url: String,
    /// Whether Chromium classified the navigation as a download.
    pub is_download: bool,
}

/// Current bounded top-level history entry used for action navigation audits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpHistoryEntry {
    /// Browser-local navigation entry identity.
    pub id: i64,
    /// Current exact entry URL.
    pub url: String,
}

/// Bounded top-level navigation history for one attached page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpNavigationHistory {
    /// Current entry index.
    pub current_index: usize,
    /// Ordered bounded entries.
    pub entries: Vec<CdpHistoryEntry>,
}

/// One exact browser frame identity and navigation instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpFrame {
    /// CDP frame identity.
    pub id: String,
    /// Exact parent frame identity; absent only for the main frame.
    pub parent_id: Option<String>,
    /// Loader identity binding the current frame navigation.
    pub loader_id: String,
    /// Browser-reported current frame URL.
    pub url: String,
    /// Browser security origin; opaque origins remain non-HTTP strings.
    pub security_origin: String,
}

/// Bounded parent-before-child frame tree for one attached page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CdpFrameTree {
    /// Exact root frame identity.
    pub main_frame_id: String,
    /// Parent-before-child frames with unique identities.
    pub frames: Vec<CdpFrame>,
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

    /// Creates one blank page inside the dedicated managed-browser profile.
    pub async fn create_blank_target(&mut self) -> Result<String, CdpTransportError> {
        let result = self
            .transport
            .call("Target.createTarget", json!({"url":"about:blank"}), None)
            .await?;
        bounded_text(&result, "targetId", 4_096)
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

    /// Navigates one attached page and waits for the exact admitted readiness event.
    pub async fn navigate(
        &mut self,
        session_id: &str,
        destination_url: &str,
        wait_until: CdpWaitUntil,
    ) -> Result<CdpNavigationResult, CdpTransportError> {
        validate_id(session_id)?;
        validate_http_url(destination_url)?;
        self.transport
            .call("Page.enable", json!({}), Some(session_id.into()))
            .await?;
        for method in [
            "Page.domContentEventFired",
            "Page.loadEventFired",
            "Page.lifecycleEvent",
            "Page.frameNavigated",
        ] {
            self.transport.discard_events(method, Some(session_id));
        }
        if wait_until == CdpWaitUntil::NetworkIdle {
            self.transport
                .call(
                    "Page.setLifecycleEventsEnabled",
                    json!({"enabled":true}),
                    Some(session_id.into()),
                )
                .await?;
        }
        let result = self
            .transport
            .call(
                "Page.navigate",
                json!({"url":destination_url}),
                Some(session_id.into()),
            )
            .await?;
        if result
            .get("errorText")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
        {
            return Err(CdpTransportError::NavigationFailed);
        }
        let frame_id = bounded_text(&result, "frameId", 4_096)?;
        match wait_until {
            CdpWaitUntil::DomContentLoaded => {
                self.transport
                    .wait_for_event("Page.domContentEventFired", Some(session_id))
                    .await?;
            }
            CdpWaitUntil::Load => {
                self.transport
                    .wait_for_event("Page.loadEventFired", Some(session_id))
                    .await?;
            }
            CdpWaitUntil::NetworkIdle => {
                self.transport
                    .wait_for_event_matching("Page.lifecycleEvent", Some(session_id), |params| {
                        params.get("frameId").and_then(Value::as_str) == Some(frame_id.as_str())
                            && params.get("name").and_then(Value::as_str) == Some("networkIdle")
                    })
                    .await?;
            }
        }
        let frame = self
            .transport
            .wait_for_event_matching("Page.frameNavigated", Some(session_id), |params| {
                params
                    .get("frame")
                    .and_then(|frame| frame.get("id"))
                    .and_then(Value::as_str)
                    == Some(frame_id.as_str())
            })
            .await?;
        let final_url = frame
            .get("frame")
            .and_then(|frame| frame.get("url"))
            .and_then(Value::as_str)
            .filter(|value| value.len() <= 32_768)
            .ok_or_else(protocol)?
            .to_owned();
        validate_http_url(&final_url)?;
        Ok(CdpNavigationResult {
            frame_id,
            loader_id: optional_object_text(
                result.as_object().ok_or_else(protocol)?,
                "loaderId",
                4_096,
            )?,
            final_url,
            is_download: result
                .get("isDownload")
                .map(|value| value.as_bool().ok_or_else(protocol))
                .transpose()?
                .unwrap_or(false),
        })
    }

    /// Returns the current bounded top-level navigation entry.
    pub async fn current_history_entry(
        &mut self,
        session_id: &str,
    ) -> Result<CdpHistoryEntry, CdpTransportError> {
        let history = self.navigation_history(session_id).await?;
        Ok(history.entries[history.current_index].clone())
    }

    /// Returns the bounded ordered top-level navigation history.
    pub async fn navigation_history(
        &mut self,
        session_id: &str,
    ) -> Result<CdpNavigationHistory, CdpTransportError> {
        validate_id(session_id)?;
        let result = self
            .transport
            .call(
                "Page.getNavigationHistory",
                json!({}),
                Some(session_id.into()),
            )
            .await?;
        parse_navigation_history(&result)
    }

    /// Returns one strictly bounded parent-before-child frame snapshot.
    pub async fn frame_tree(
        &mut self,
        session_id: &str,
    ) -> Result<CdpFrameTree, CdpTransportError> {
        validate_id(session_id)?;
        let result = self
            .transport
            .call("Page.getFrameTree", json!({}), Some(session_id.into()))
            .await?;
        parse_frame_tree(&result)
    }

    /// Resolves one exact child frame to its embedding backend DOM node.
    pub async fn frame_owner_backend_node(
        &mut self,
        session_id: &str,
        frame_id: &str,
    ) -> Result<u64, CdpTransportError> {
        validate_id(session_id)?;
        validate_id(frame_id)?;
        let result = self
            .transport
            .call(
                "DOM.getFrameOwner",
                json!({"frameId":frame_id}),
                Some(session_id.into()),
            )
            .await?;
        result
            .get("backendNodeId")
            .and_then(Value::as_u64)
            .filter(|identity| *identity > 0)
            .ok_or_else(protocol)
    }

    /// Moves to one exact history entry and proves it became current.
    pub async fn navigate_to_history_entry(
        &mut self,
        session_id: &str,
        entry_id: i64,
    ) -> Result<CdpHistoryEntry, CdpTransportError> {
        validate_id(session_id)?;
        if entry_id < 0 {
            return Err(protocol());
        }
        self.transport
            .call(
                "Page.navigateToHistoryEntry",
                json!({"entryId":entry_id}),
                Some(session_id.into()),
            )
            .await?;
        for _ in 0..16 {
            let current = self.current_history_entry(session_id).await?;
            if current.id == entry_id {
                return Ok(current);
            }
            tokio::task::yield_now().await;
        }
        Err(CdpTransportError::Timeout)
    }

    /// Reloads the attached page and waits for a fresh load event.
    pub async fn reload(&mut self, session_id: &str) -> Result<CdpHistoryEntry, CdpTransportError> {
        validate_id(session_id)?;
        self.transport
            .call("Page.enable", json!({}), Some(session_id.into()))
            .await?;
        self.transport
            .discard_events("Page.loadEventFired", Some(session_id));
        self.transport
            .call(
                "Page.reload",
                json!({"ignoreCache":false}),
                Some(session_id.into()),
            )
            .await?;
        self.transport
            .wait_for_event("Page.loadEventFired", Some(session_id))
            .await?;
        self.current_history_entry(session_id).await
    }

    /// Clicks one adapter-private backend node through its current rendered box.
    pub async fn click_backend_node(
        &mut self,
        session_id: &str,
        backend_dom_node_id: u64,
    ) -> Result<(), CdpTransportError> {
        validate_id(session_id)?;
        if backend_dom_node_id == 0 {
            return Err(protocol());
        }
        self.transport
            .call(
                "DOM.scrollIntoViewIfNeeded",
                json!({"backendNodeId":backend_dom_node_id}),
                Some(session_id.into()),
            )
            .await?;
        let result = self
            .transport
            .call(
                "DOM.getBoxModel",
                json!({"backendNodeId":backend_dom_node_id}),
                Some(session_id.into()),
            )
            .await?;
        let (x, y) = content_center(&result)?;
        for params in [
            json!({"type":"mouseMoved","x":x,"y":y,"button":"none","buttons":0}),
            json!({"type":"mousePressed","x":x,"y":y,"button":"left","buttons":1,"clickCount":1}),
            json!({"type":"mouseReleased","x":x,"y":y,"button":"left","buttons":0,"clickCount":1}),
        ] {
            self.transport
                .call("Input.dispatchMouseEvent", params, Some(session_id.into()))
                .await?;
        }
        Ok(())
    }

    /// Focuses one adapter-private backend node and inserts bounded UTF-8 text.
    pub async fn type_text_backend_node(
        &mut self,
        session_id: &str,
        backend_dom_node_id: u64,
        text: &str,
    ) -> Result<(), CdpTransportError> {
        if text.len() > MAX_ACTION_TEXT_BYTES {
            return Err(protocol());
        }
        self.focus_backend_node(session_id, backend_dom_node_id)
            .await?;
        self.transport
            .call(
                "Input.insertText",
                json!({"text":text}),
                Some(session_id.into()),
            )
            .await?;
        Ok(())
    }

    /// Clears one editable adapter-private backend node without clipboard access.
    pub async fn clear_backend_node(
        &mut self,
        session_id: &str,
        backend_dom_node_id: u64,
    ) -> Result<(), CdpTransportError> {
        self.focus_backend_node(session_id, backend_dom_node_id)
            .await?;
        for params in [
            json!({"type":"rawKeyDown","commands":["selectAll"]}),
            json!({"type":"rawKeyDown","key":"Backspace","code":"Backspace","windowsVirtualKeyCode":8}),
            json!({"type":"keyUp","key":"Backspace","code":"Backspace","windowsVirtualKeyCode":8}),
        ] {
            self.transport
                .call("Input.dispatchKeyEvent", params, Some(session_id.into()))
                .await?;
        }
        Ok(())
    }

    /// Selects one unique enabled native option through a fixed adapter function.
    pub async fn select_option_backend_node(
        &mut self,
        session_id: &str,
        backend_dom_node_id: u64,
        option: &str,
    ) -> Result<CdpSelectOutcome, CdpTransportError> {
        validate_id(session_id)?;
        if backend_dom_node_id == 0
            || option.is_empty()
            || option.chars().count() > MAX_OPTION_SCALARS
            || option.len() > MAX_OPTION_UTF8_BYTES
        {
            return Err(protocol());
        }
        let resolved = self
            .transport
            .call(
                "DOM.resolveNode",
                json!({"backendNodeId":backend_dom_node_id}),
                Some(session_id.into()),
            )
            .await?;
        let object_id = resolved
            .get("object")
            .and_then(Value::as_object)
            .and_then(|object| object.get("objectId"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 4_096)
            .ok_or_else(protocol)?
            .to_owned();
        let selection = self
            .transport
            .call(
                "Runtime.callFunctionOn",
                json!({
                    "objectId":object_id,
                    "functionDeclaration":SELECT_OPTION_FUNCTION,
                    "arguments":[{"value":option}],
                    "returnByValue":true,
                    "awaitPromise":false,
                    "userGesture":true
                }),
                Some(session_id.into()),
            )
            .await;
        let release = self
            .transport
            .call(
                "Runtime.releaseObject",
                json!({"objectId":object_id}),
                Some(session_id.into()),
            )
            .await;
        let selection = selection?;
        release?;
        if selection.get("exceptionDetails").is_some() {
            return Err(protocol());
        }
        let result = selection
            .get("result")
            .and_then(Value::as_object)
            .and_then(|result| result.get("value"))
            .and_then(Value::as_object)
            .ok_or_else(protocol)?;
        match result.get("status").and_then(Value::as_str) {
            Some("selected") if result.get("value").and_then(Value::as_str) == Some(option) => {
                Ok(CdpSelectOutcome::Selected {
                    changed: result
                        .get("changed")
                        .and_then(Value::as_bool)
                        .ok_or_else(protocol)?,
                })
            }
            Some("unavailable") => Ok(CdpSelectOutcome::OptionUnavailable),
            _ => Err(protocol()),
        }
    }

    /// Dispatches one closed portable key down/up pair to the current page focus.
    pub async fn press_key(
        &mut self,
        session_id: &str,
        key: CdpPortableKey,
    ) -> Result<(), CdpTransportError> {
        validate_id(session_id)?;
        let (key, code, virtual_key, text) = portable_key_fields(key);
        let mut down = json!({
            "type":"rawKeyDown",
            "key":key,
            "code":code,
            "windowsVirtualKeyCode":virtual_key
        });
        if let Some(text) = text {
            down["type"] = json!("keyDown");
            down["text"] = json!(text);
            down["unmodifiedText"] = json!(text);
        }
        self.transport
            .call("Input.dispatchKeyEvent", down, Some(session_id.into()))
            .await?;
        self.transport
            .call(
                "Input.dispatchKeyEvent",
                json!({
                    "type":"keyUp",
                    "key":key,
                    "code":code,
                    "windowsVirtualKeyCode":virtual_key
                }),
                Some(session_id.into()),
            )
            .await?;
        Ok(())
    }

    /// Scrolls the current visual viewport from its browser-reported center.
    pub async fn scroll_viewport(
        &mut self,
        session_id: &str,
        delta_x: i64,
        delta_y: i64,
    ) -> Result<(), CdpTransportError> {
        validate_id(session_id)?;
        if (delta_x == 0 && delta_y == 0)
            || delta_x.unsigned_abs() > MAX_SCROLL_DELTA
            || delta_y.unsigned_abs() > MAX_SCROLL_DELTA
        {
            return Err(protocol());
        }
        let before = self.viewport_metrics(session_id).await?;
        self.transport
            .call(
                "Input.dispatchMouseEvent",
                json!({
                    "type":"mouseWheel",
                    "x":before.client_width / 2.0,
                    "y":before.client_height / 2.0,
                    "deltaX":delta_x,
                    "deltaY":delta_y,
                    "button":"none",
                    "buttons":0
                }),
                Some(session_id.into()),
            )
            .await?;
        if !before.can_move(delta_x, delta_y) {
            return Ok(());
        }
        for _ in 0..SCROLL_SETTLE_ATTEMPTS {
            tokio::time::sleep(SCROLL_SETTLE_INTERVAL).await;
            let after = self.viewport_metrics(session_id).await?;
            if before.position_changed(after) {
                return Ok(());
            }
        }
        Err(CdpTransportError::Timeout)
    }

    async fn viewport_metrics(
        &mut self,
        session_id: &str,
    ) -> Result<CdpViewportMetrics, CdpTransportError> {
        let result = self
            .transport
            .call("Page.getLayoutMetrics", json!({}), Some(session_id.into()))
            .await?;
        parse_viewport_metrics(&result)
    }

    /// Returns the one focused AX backend node under explicit observation bounds.
    pub async fn focused_backend_node(
        &mut self,
        session_id: &str,
        depth: u32,
        max_nodes: usize,
        max_text_bytes: usize,
    ) -> Result<Option<u64>, CdpTransportError> {
        let tree = self
            .full_ax_tree(session_id, None, depth, max_nodes, max_text_bytes)
            .await?;
        let focused = tree
            .nodes
            .iter()
            .filter(|node| {
                node.properties.iter().any(|property| {
                    property.name.eq_ignore_ascii_case("focused")
                        && property_truthy(&property.value)
                })
            })
            .collect::<Vec<_>>();
        let by_id = tree
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        if by_id.len() != tree.nodes.len() {
            return Err(protocol());
        }
        let deepest = focused
            .iter()
            .filter_map(|candidate| {
                let has_focused_descendant = focused.iter().try_fold(false, |found, other| {
                    if found || candidate.node_id == other.node_id {
                        Ok(found)
                    } else {
                        raw_ax_ancestor(candidate.node_id.as_str(), other, &by_id)
                    }
                });
                match has_focused_descendant {
                    Ok(false) => Some(Ok(*candidate)),
                    Ok(true) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .collect::<Result<Vec<_>, CdpTransportError>>()?;
        if deepest.len() > 1 {
            return Err(protocol());
        }
        deepest
            .first()
            .map(|node| node.backend_dom_node_id.ok_or_else(protocol))
            .transpose()
    }

    async fn focus_backend_node(
        &mut self,
        session_id: &str,
        backend_dom_node_id: u64,
    ) -> Result<(), CdpTransportError> {
        validate_id(session_id)?;
        if backend_dom_node_id == 0 {
            return Err(protocol());
        }
        self.transport
            .call(
                "DOM.focus",
                json!({"backendNodeId":backend_dom_node_id}),
                Some(session_id.into()),
            )
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

fn parse_frame_tree(result: &Value) -> Result<CdpFrameTree, CdpTransportError> {
    let root = result.get("frameTree").ok_or_else(protocol)?;
    let mut pending = vec![(root, None::<String>, 0_usize)];
    let mut frames = Vec::new();
    let mut identities = BTreeSet::new();
    let mut text_bytes = 0_usize;
    while let Some((tree, expected_parent, depth)) = pending.pop() {
        if depth > MAX_FRAME_DEPTH || frames.len() >= MAX_FRAME_COUNT {
            return Err(protocol());
        }
        let tree = tree.as_object().ok_or_else(protocol)?;
        let frame = tree
            .get("frame")
            .and_then(Value::as_object)
            .ok_or_else(protocol)?;
        let id = object_text(frame, "id", 4_096)?;
        if !identities.insert(id.clone()) {
            return Err(protocol());
        }
        let declared_parent = optional_object_text(frame, "parentId", 4_096)?;
        if declared_parent != expected_parent {
            return Err(protocol());
        }
        let loader_id = object_text(frame, "loaderId", 4_096)?;
        let url = object_text(frame, "url", 32_768)?;
        let security_origin = object_text(frame, "securityOrigin", 4_096)?;
        text_bytes = text_bytes
            .checked_add(id.len())
            .and_then(|count| count.checked_add(loader_id.len()))
            .and_then(|count| count.checked_add(url.len()))
            .and_then(|count| count.checked_add(security_origin.len()))
            .and_then(|count| count.checked_add(declared_parent.as_ref().map_or(0, String::len)))
            .filter(|count| *count <= MAX_FRAME_TEXT_BYTES)
            .ok_or_else(protocol)?;
        frames.push(CdpFrame {
            id: id.clone(),
            parent_id: declared_parent,
            loader_id,
            url,
            security_origin,
        });
        let children = tree
            .get("childFrames")
            .map(|value| value.as_array().ok_or_else(protocol))
            .transpose()?
            .map(Vec::as_slice)
            .unwrap_or_default();
        if frames
            .len()
            .saturating_add(pending.len())
            .saturating_add(children.len())
            > MAX_FRAME_COUNT
        {
            return Err(protocol());
        }
        for child in children.iter().rev() {
            pending.push((child, Some(id.clone()), depth + 1));
        }
    }
    let main_frame_id = frames
        .first()
        .map(|frame| frame.id.clone())
        .ok_or_else(protocol)?;
    Ok(CdpFrameTree {
        main_frame_id,
        frames,
    })
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

fn validate_http_url(value: &str) -> Result<(), CdpTransportError> {
    let url = Url::parse(value).map_err(|_| protocol())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        Err(protocol())
    } else {
        Ok(())
    }
}

fn content_center(result: &Value) -> Result<(f64, f64), CdpTransportError> {
    let content = result
        .get("model")
        .and_then(|model| model.get("content"))
        .and_then(Value::as_array)
        .filter(|values| values.len() == 8)
        .ok_or_else(protocol)?;
    let coordinates = content
        .iter()
        .map(|value| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && value.abs() <= 10_000_000.0)
                .ok_or_else(protocol)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let x = (coordinates[0] + coordinates[2] + coordinates[4] + coordinates[6]) / 4.0;
    let y = (coordinates[1] + coordinates[3] + coordinates[5] + coordinates[7]) / 4.0;
    if x.is_finite() && y.is_finite() {
        Ok((x, y))
    } else {
        Err(protocol())
    }
}

fn portable_key_fields(
    key: CdpPortableKey,
) -> (&'static str, &'static str, u16, Option<&'static str>) {
    match key {
        CdpPortableKey::Enter => ("Enter", "Enter", 13, Some("\r")),
        CdpPortableKey::Tab => ("Tab", "Tab", 9, None),
        CdpPortableKey::Escape => ("Escape", "Escape", 27, None),
        CdpPortableKey::Backspace => ("Backspace", "Backspace", 8, None),
        CdpPortableKey::Delete => ("Delete", "Delete", 46, None),
        CdpPortableKey::ArrowUp => ("ArrowUp", "ArrowUp", 38, None),
        CdpPortableKey::ArrowDown => ("ArrowDown", "ArrowDown", 40, None),
        CdpPortableKey::ArrowLeft => ("ArrowLeft", "ArrowLeft", 37, None),
        CdpPortableKey::ArrowRight => ("ArrowRight", "ArrowRight", 39, None),
        CdpPortableKey::Home => ("Home", "Home", 36, None),
        CdpPortableKey::End => ("End", "End", 35, None),
        CdpPortableKey::PageUp => ("PageUp", "PageUp", 33, None),
        CdpPortableKey::PageDown => ("PageDown", "PageDown", 34, None),
        CdpPortableKey::Space => (" ", "Space", 32, Some(" ")),
    }
}

fn optional_len(value: &Option<String>) -> usize {
    value.as_ref().map_or(0, String::len)
}

fn parse_navigation_history(result: &Value) -> Result<CdpNavigationHistory, CdpTransportError> {
    let entries = result
        .get("entries")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty() && entries.len() <= 10_000)
        .ok_or_else(protocol)?;
    let current_index = result
        .get("currentIndex")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < entries.len())
        .ok_or_else(protocol)?;
    let entries = entries
        .iter()
        .map(|entry| {
            let entry = entry.as_object().ok_or_else(protocol)?;
            Ok(CdpHistoryEntry {
                id: entry
                    .get("id")
                    .and_then(Value::as_i64)
                    .filter(|id| *id >= 0)
                    .ok_or_else(protocol)?,
                url: object_text(entry, "url", 32_768)?,
            })
        })
        .collect::<Result<Vec<_>, CdpTransportError>>()?;
    Ok(CdpNavigationHistory {
        current_index,
        entries,
    })
}

fn property_truthy(value: &Value) -> bool {
    value.get("value").is_some_and(|value| match value {
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty() && value != "false",
        Value::Number(value) => value.as_i64() != Some(0),
        _ => false,
    })
}

fn raw_ax_ancestor(
    candidate: &str,
    descendant: &CdpAxNode,
    by_id: &BTreeMap<&str, &CdpAxNode>,
) -> Result<bool, CdpTransportError> {
    let mut current = descendant.parent_id.as_deref();
    let mut visited = BTreeSet::new();
    while let Some(parent) = current {
        if !visited.insert(parent) {
            return Err(protocol());
        }
        if parent == candidate {
            return Ok(true);
        }
        current = by_id.get(parent).ok_or_else(protocol)?.parent_id.as_deref();
    }
    Ok(false)
}

fn parse_viewport_metrics(value: &Value) -> Result<CdpViewportMetrics, CdpTransportError> {
    let viewport = value
        .get("visualViewport")
        .and_then(Value::as_object)
        .ok_or_else(protocol)?;
    let content = value
        .get("contentSize")
        .and_then(Value::as_object)
        .ok_or_else(protocol)?;
    let coordinate = |object: &Map<String, Value>, field| {
        object
            .get(field)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0 && *value <= MAX_LAYOUT_EXTENT)
            .ok_or_else(protocol)
    };
    let extent = |object: &Map<String, Value>, field| {
        coordinate(object, field)
            .and_then(|value| (value > 0.0).then_some(value).ok_or_else(protocol))
    };
    Ok(CdpViewportMetrics {
        page_x: coordinate(viewport, "pageX")?,
        page_y: coordinate(viewport, "pageY")?,
        client_width: extent(viewport, "clientWidth")?,
        client_height: extent(viewport, "clientHeight")?,
        content_width: extent(content, "width")?,
        content_height: extent(content, "height")?,
    })
}

fn protocol() -> CdpTransportError {
    CdpTransportError::Protocol(CdpProtocolError::InvalidMessage)
}
