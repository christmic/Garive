//! Bounded loopback HTTP/SSE Host client.

use futures::StreamExt;
use reqwest::{redirect::Policy, StatusCode, Url};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tokio::time::{timeout, Duration};

use crate::{
    reduce_host_events, reducer::validate_activity, AgentDefinitionPage, ClientLimits,
    CreateSessionResponse, HostClientError, HostClientErrorCode, HostEvent, HostView, SessionPage,
    SessionSummary, SessionView, SuspensionView, TurnCommandResponse, TurnTimelinePage,
};

const KNOWN_HOST_ERRORS: [&str; 8] = [
    "invalid_request",
    "not_found",
    "command_conflict",
    "concurrent_modification",
    "precondition_failed",
    "durability_unavailable",
    "corrupt_state",
    "read_bound_exceeded",
];

/// Explicit loopback implementation of the A1 Host client boundary.
#[derive(Clone)]
pub struct LiveHostClient {
    base_url: Url,
    limits: ClientLimits,
    http: reqwest::Client,
}

impl std::fmt::Debug for LiveHostClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveHostClient")
            .field("origin", &self.base_url.origin().ascii_serialization())
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl LiveHostClient {
    /// Constructs a bounded client from an explicit loopback root URL.
    pub fn new(base_url: &str, limits: ClientLimits) -> Result<Self, HostClientError> {
        if limits.max_command_bytes == 0
            || limits.max_event_bytes == 0
            || limits.max_events == 0
            || limits.follow_deadline_ms == 0
        {
            return Err(HostClientError::new(
                HostClientErrorCode::InvalidConfiguration,
            ));
        }
        let base_url = validate_base_url(base_url)?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .build()
            .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidConfiguration))?;
        Ok(Self {
            base_url,
            limits,
            http,
        })
    }

    /// Lists the bounded installed Agent definitions exposed by the Host.
    pub async fn list_agent_definitions(&self) -> Result<AgentDefinitionPage, HostClientError> {
        let page: AgentDefinitionPage = decode(self.get("v1/agent-definitions").await?)?;
        if page.api_version != "v1"
            || page.definitions.is_empty()
            || page.definitions.len() > self.limits.max_events
            || page.definitions.iter().any(|item| {
                item.api_version != "v1"
                    || item.definition_id.is_empty()
                    || item.definition_revision.is_empty()
                    || !is_sorted_unique(&item.capabilities)
            })
        {
            return Err(invalid_event());
        }
        Ok(page)
    }

    /// Lists one reverse-opened bounded page of durable Sessions.
    pub async fn list_sessions(
        &self,
        limit: usize,
        before: Option<&str>,
    ) -> Result<SessionPage, HostClientError> {
        if limit == 0
            || limit > self.limits.max_events
            || before.is_some_and(|value| {
                value.is_empty()
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
        {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let mut path = format!("v1/sessions?limit={limit}");
        if let Some(cursor) = before {
            path.push_str("&before=");
            path.push_str(cursor);
        }
        let page: SessionPage = decode(self.get(&path).await?)?;
        if page.api_version != "v1"
            || page.sessions.len() > limit
            || page.sessions.iter().any(|session| !valid_session(session))
            || page.next_before.as_deref().is_some_and(|value| {
                value.is_empty()
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
        {
            return Err(invalid_event());
        }
        Ok(page)
    }

    /// Reads one exact durable Session summary.
    pub async fn get_session(&self, session_id: &str) -> Result<SessionView, HostClientError> {
        if session_id.is_empty() {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let path = format!("v1/sessions/{}", encode_segment(session_id));
        let view: SessionView = decode(self.get(&path).await?)?;
        if view.api_version != "v1"
            || view.session.session_id != session_id
            || view.observed_max_position != view.session.latest_position
            || !valid_session(&view.session)
        {
            return Err(invalid_event());
        }
        Ok(view)
    }

    /// Reads one bounded page of complete durable Turn projections.
    pub async fn get_timeline(
        &self,
        session_id: &str,
        after_position: u64,
        limit: usize,
    ) -> Result<TurnTimelinePage, HostClientError> {
        if session_id.is_empty() || limit == 0 || limit > self.limits.max_events {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let path = format!(
            "v1/sessions/{}/timeline?after_position={after_position}&limit={limit}",
            encode_segment(session_id)
        );
        let page: TurnTimelinePage = decode(self.get(&path).await?)?;
        if page.api_version != "v1"
            || page.session_id != session_id
            || page.observed_max_position == 0
            || page.scanned_through_position < after_position
            || page.scanned_through_position > page.observed_max_position
            || page.items.len() > limit
            || page.items.iter().any(|item| {
                item.turn_id.is_empty()
                    || item.started_position == 0
                    || item.latest_position < item.started_position
                    || item.latest_position > page.scanned_through_position
                    || !valid_state(&item.state)
                    || (item.state == "completed") != item.completion_text.is_some()
                    || (item.state == "suspended") != item.suspension.is_some()
                    || item
                        .suspension
                        .as_ref()
                        .is_some_and(|value| !valid_suspension(value))
                    || item
                        .activities
                        .iter()
                        .any(|activity| validate_activity(activity, item.latest_position).is_err())
                    || item.activities.iter().enumerate().any(|(index, activity)| {
                        item.activities[..index]
                            .iter()
                            .any(|prior| prior.activity_id == activity.activity_id)
                    })
            })
            || page
                .items
                .windows(2)
                .any(|items| items[0].latest_position >= items[1].latest_position)
            || (page.has_more && page.items.is_empty())
            || (!page.has_more && page.scanned_through_position != page.observed_max_position)
        {
            return Err(invalid_event());
        }
        Ok(page)
    }

    /// Creates a Session using a caller-owned stable command identity.
    pub async fn create_session(
        &self,
        command_id: &str,
        definition_id: &str,
    ) -> Result<CreateSessionResponse, HostClientError> {
        if definition_id.is_empty() {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let value = self
            .post(
                "v1/sessions",
                command_id,
                &SessionCommand {
                    agent_definition_id: definition_id,
                },
            )
            .await?;
        validate_session_response(value)
    }

    /// Starts one Turn using a caller-owned stable command identity.
    pub async fn start_turn(
        &self,
        command_id: &str,
        session_id: &str,
        text: &str,
    ) -> Result<TurnCommandResponse, HostClientError> {
        if session_id.is_empty() || text.is_empty() {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let path = format!("v1/sessions/{}/turns", encode_segment(session_id));
        let value = self.post(&path, command_id, &TurnCommand { text }).await?;
        let response = validate_turn_response(value)?;
        if response.session_id != session_id {
            return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
        }
        Ok(response)
    }

    /// Requests durable Turn cancellation through an observed position.
    pub async fn cancel_turn(
        &self,
        command_id: &str,
        session_id: &str,
        turn_id: &str,
        requested_through_position: u64,
    ) -> Result<TurnCommandResponse, HostClientError> {
        if session_id.is_empty() || turn_id.is_empty() || requested_through_position == 0 {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let path = format!("v1/turns/{}:cancel", encode_segment(turn_id));
        let value = self
            .post(
                &path,
                command_id,
                &CancelCommand {
                    session_id,
                    requested_through_position,
                },
            )
            .await?;
        validate_owned_turn_response(value, session_id, turn_id)
    }

    /// Continues one exact durable suspension with caller-supplied input.
    pub async fn continue_turn(
        &self,
        command_id: &str,
        session_id: &str,
        turn_id: &str,
        suspension_id: &str,
        expected_session_version: u64,
        input: &str,
    ) -> Result<TurnCommandResponse, HostClientError> {
        if session_id.is_empty()
            || turn_id.is_empty()
            || suspension_id.is_empty()
            || expected_session_version == 0
            || input.is_empty()
        {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let path = format!("v1/turns/{}:continue", encode_segment(turn_id));
        let value = self
            .post(
                &path,
                command_id,
                &ContinueCommand {
                    session_id,
                    suspension_id,
                    expected_session_version,
                    input,
                },
            )
            .await?;
        validate_owned_turn_response(value, session_id, turn_id)
    }

    /// Continues one typed interaction with exact RFC 8785 JSON text.
    pub async fn continue_turn_json(
        &self,
        command_id: &str,
        session_id: &str,
        turn_id: &str,
        suspension_id: &str,
        expected_session_version: u64,
        input_json: &str,
    ) -> Result<TurnCommandResponse, HostClientError> {
        if session_id.is_empty()
            || turn_id.is_empty()
            || suspension_id.is_empty()
            || expected_session_version == 0
        {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let parsed: Value = serde_json::from_str(input_json)
            .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidCommand))?;
        let canonical = serde_jcs::to_string(&parsed)
            .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidCommand))?;
        if canonical != input_json {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let path = format!("v1/turns/{}:continue", encode_segment(turn_id));
        let value = self
            .post(
                &path,
                command_id,
                &ContinueJsonCommand {
                    session_id,
                    suspension_id,
                    expected_session_version,
                    input_json,
                },
            )
            .await?;
        validate_owned_turn_response(value, session_id, turn_id)
    }

    /// Follows committed events until an explicit durable terminal event.
    pub async fn follow_until_terminal(
        &self,
        session_id: &str,
        after_position: u64,
    ) -> Result<HostView, HostClientError> {
        self.follow_until_terminal_with(session_id, after_position, |_| {})
            .await
    }

    /// Follows validated semantic events into a bounded asynchronous sink.
    pub async fn follow_events(
        &self,
        session_id: &str,
        after_position: u64,
        sink: tokio::sync::mpsc::Sender<HostEvent>,
    ) -> Result<(), HostClientError> {
        if session_id.is_empty() {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let operation = self.follow_events_inner(session_id, after_position, sink);
        timeout(
            Duration::from_millis(self.limits.follow_deadline_ms),
            operation,
        )
        .await
        .map_err(|_| HostClientError::new(HostClientErrorCode::FollowDeadline))?
    }

    async fn follow_events_inner(
        &self,
        session_id: &str,
        after_position: u64,
        sink: tokio::sync::mpsc::Sender<HostEvent>,
    ) -> Result<(), HostClientError> {
        let path = format!(
            "v1/sessions/{}/events?after_position={after_position}",
            encode_segment(session_id)
        );
        let response = self
            .http
            .get(self.join(&path)?)
            .send()
            .await
            .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(HostClientError::new(HostClientErrorCode::TransportFailure));
        }
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
            return Err(classify_host_error(status, &bytes));
        }
        if !response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().starts_with("text/event-stream"))
        {
            return Err(HostClientError::new(HostClientErrorCode::TransportFailure));
        }
        let mut stream = response.bytes_stream();
        let mut pending = Vec::new();
        let mut count = 0usize;
        let mut cursor = after_position;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
            pending.extend_from_slice(&chunk);
            if pending.len() > self.limits.max_event_bytes.saturating_mul(2) {
                return Err(HostClientError::new(
                    HostClientErrorCode::EventLimitExceeded,
                ));
            }
            while let Some(boundary) = find_sse_boundary(&pending) {
                let block: Vec<u8> = pending.drain(..boundary).collect();
                let separator = if pending.starts_with(b"\r\n\r\n") {
                    4
                } else {
                    2
                };
                pending.drain(..separator);
                let Some(data) = sse_data(&block, self.limits.max_event_bytes)? else {
                    continue;
                };
                count += 1;
                if count > self.limits.max_events {
                    return Err(HostClientError::new(
                        HostClientErrorCode::EventLimitExceeded,
                    ));
                }
                let event: HostEvent = serde_json::from_slice(&data)
                    .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidEvent))?;
                if event.api_version != "v1"
                    || event.session_id != session_id
                    || event.position <= cursor
                    || match (&event.activity, event.event.starts_with("agent.activity.")) {
                        (Some(activity), true) => {
                            validate_activity(activity, event.position).is_err()
                        }
                        (None, false) => false,
                        _ => true,
                    }
                {
                    return Err(HostClientError::new(
                        HostClientErrorCode::EventOrderViolation,
                    ));
                }
                cursor = event.position;
                sink.send(event)
                    .await
                    .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
            }
        }
        Err(HostClientError::new(HostClientErrorCode::TransportFailure))
    }

    /// Follows until terminal and observes each newly applied durable event.
    pub async fn follow_until_terminal_with<F>(
        &self,
        session_id: &str,
        after_position: u64,
        observer: F,
    ) -> Result<HostView, HostClientError>
    where
        F: FnMut(&HostEvent),
    {
        if session_id.is_empty() {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let operation = self.follow(session_id, after_position, observer);
        timeout(
            Duration::from_millis(self.limits.follow_deadline_ms),
            operation,
        )
        .await
        .map_err(|_| HostClientError::new(HostClientErrorCode::FollowDeadline))?
    }

    async fn follow<F>(
        &self,
        session_id: &str,
        after_position: u64,
        mut observer: F,
    ) -> Result<HostView, HostClientError>
    where
        F: FnMut(&HostEvent),
    {
        let path = format!(
            "v1/sessions/{}/events?after_position={after_position}",
            encode_segment(session_id)
        );
        let response = self
            .http
            .get(self.join(&path)?)
            .send()
            .await
            .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(HostClientError::new(HostClientErrorCode::TransportFailure));
        }
        if !status.is_success() {
            let bytes = response
                .bytes()
                .await
                .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
            if bytes.len() > self.limits.max_event_bytes {
                return Err(HostClientError::with_status(
                    HostClientErrorCode::UnknownHostError,
                    status.as_u16(),
                ));
            }
            return Err(classify_host_error(status, &bytes));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.starts_with("text/event-stream") {
            return Err(HostClientError::new(HostClientErrorCode::TransportFailure));
        }
        let mut stream = response.bytes_stream();
        let mut pending = Vec::new();
        let mut count = 0usize;
        let mut view = HostView::at_cursor(after_position);
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
            pending.extend_from_slice(&chunk);
            if pending.len() > self.limits.max_event_bytes.saturating_mul(2) {
                return Err(HostClientError::new(
                    HostClientErrorCode::EventLimitExceeded,
                ));
            }
            while let Some(boundary) = find_sse_boundary(&pending) {
                let block: Vec<u8> = pending.drain(..boundary).collect();
                let separator = if pending.starts_with(b"\r\n\r\n") {
                    4
                } else {
                    2
                };
                pending.drain(..separator);
                let Some(data) = sse_data(&block, self.limits.max_event_bytes)? else {
                    continue;
                };
                count += 1;
                if count > self.limits.max_events {
                    return Err(HostClientError::new(
                        HostClientErrorCode::EventLimitExceeded,
                    ));
                }
                let event: HostEvent = serde_json::from_slice(&data)
                    .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidEvent))?;
                let previous_cursor = view.cursor;
                view = reduce_host_events(
                    session_id,
                    std::slice::from_ref(&event),
                    view,
                    self.limits.max_events,
                )?;
                if view.cursor != previous_cursor {
                    observer(&event);
                }
                if view.terminal.is_some() {
                    return Ok(view);
                }
            }
        }
        Err(HostClientError::new(HostClientErrorCode::TransportFailure))
    }

    async fn post<T: Serialize>(
        &self,
        path: &str,
        command_id: &str,
        body: &T,
    ) -> Result<Value, HostClientError> {
        if !valid_command_id(command_id) {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let encoded = serde_json::to_vec(body)
            .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidCommand))?;
        if encoded.len() > self.limits.max_command_bytes {
            return Err(HostClientError::new(HostClientErrorCode::InvalidCommand));
        }
        let response = self
            .http
            .post(self.join(path)?)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", command_id)
            .body(encoded)
            .send()
            .await
            .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(HostClientError::new(HostClientErrorCode::TransportFailure));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
        if !status.is_success() {
            return Err(classify_host_error(status, &bytes));
        }
        if bytes.len() > self.limits.max_event_bytes {
            return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidEvent))
    }

    async fn get(&self, path: &str) -> Result<Value, HostClientError> {
        let response = self
            .http
            .get(self.join(path)?)
            .send()
            .await
            .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(HostClientError::new(HostClientErrorCode::TransportFailure));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| HostClientError::new(HostClientErrorCode::TransportFailure))?;
        if !status.is_success() {
            return Err(classify_host_error(status, &bytes));
        }
        if bytes.len() > self.limits.max_event_bytes {
            return Err(invalid_event());
        }
        serde_json::from_slice(&bytes).map_err(|_| invalid_event())
    }

    fn join(&self, path: &str) -> Result<Url, HostClientError> {
        self.base_url
            .join(path)
            .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidCommand))
    }
}

#[derive(Serialize)]
struct SessionCommand<'a> {
    agent_definition_id: &'a str,
}

#[derive(Serialize)]
struct TurnCommand<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct CancelCommand<'a> {
    session_id: &'a str,
    requested_through_position: u64,
}

#[derive(Serialize)]
struct ContinueCommand<'a> {
    session_id: &'a str,
    suspension_id: &'a str,
    expected_session_version: u64,
    input: &'a str,
}

#[derive(Serialize)]
struct ContinueJsonCommand<'a> {
    session_id: &'a str,
    suspension_id: &'a str,
    expected_session_version: u64,
    input_json: &'a str,
}

fn validate_base_url(value: &str) -> Result<Url, HostClientError> {
    let url = Url::parse(value)
        .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidConfiguration))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "http"
        || !loopback
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(HostClientError::new(
            HostClientErrorCode::InvalidConfiguration,
        ));
    }
    Ok(url)
}

fn valid_command_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn validate_session_response(value: Value) -> Result<CreateSessionResponse, HostClientError> {
    let response: CreateSessionResponse = decode(value)?;
    if response.session_id.is_empty()
        || response.agent_instance_id.is_empty()
        || response.committed_position == 0
    {
        return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
    }
    Ok(response)
}

fn validate_turn_response(value: Value) -> Result<TurnCommandResponse, HostClientError> {
    let response: TurnCommandResponse = decode(value)?;
    if response.session_id.is_empty()
        || response.turn_id.is_empty()
        || response.execution_id.is_empty()
        || response.committed_position == 0
    {
        return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
    }
    Ok(response)
}

fn validate_owned_turn_response(
    value: Value,
    session_id: &str,
    turn_id: &str,
) -> Result<TurnCommandResponse, HostClientError> {
    let response = validate_turn_response(value)?;
    if response.session_id != session_id || response.turn_id != turn_id {
        return Err(HostClientError::new(HostClientErrorCode::InvalidEvent));
    }
    Ok(response)
}

fn valid_session(value: &SessionSummary) -> bool {
    value.api_version == "v1"
        && !value.session_id.is_empty()
        && !value.agent_instance_id.is_empty()
        && !value.definition_id.is_empty()
        && !value.definition_revision.is_empty()
        && !value.opened_at.is_empty()
        && value.latest_position > 0
        && match (
            value.turn_count,
            value.latest_turn_id.as_deref(),
            value.latest_turn_state.as_deref(),
        ) {
            (0, None, None) => true,
            (count, Some(id), Some(state)) => count > 0 && !id.is_empty() && valid_state(state),
            _ => false,
        }
}

fn valid_state(value: &str) -> bool {
    matches!(
        value,
        "running" | "suspended" | "completed" | "stopped" | "failed"
    )
}

fn valid_suspension(value: &SuspensionView) -> bool {
    !value.suspension_id.is_empty()
        && value.session_version > 0
        && matches!(
            value.kind.as_str(),
            "approval_required"
                | "external_input_required"
                | "operator_reconciliation"
                | "resource_unavailable"
                | "partial_output"
                | "delegation_pending"
        )
        && value.prompt_schema == "garive.public-suspension-prompt.v1"
        && !value.prompt_json.is_empty()
        && valid_digest(&value.prompt_digest)
        && match (
            value.response_schema_json.as_deref(),
            value.response_schema_digest.as_deref(),
        ) {
            (Some(schema), Some(digest)) => {
                matches!(
                    value.kind.as_str(),
                    "approval_required" | "external_input_required"
                ) && !schema.is_empty()
                    && valid_digest(digest)
            }
            (None, None) => !matches!(
                value.kind.as_str(),
                "approval_required" | "external_input_required"
            ),
            _ => false,
        }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_sorted_unique(values: &[String]) -> bool {
    values
        .windows(2)
        .all(|pair| pair[0].as_str() < pair[1].as_str())
}

fn invalid_event() -> HostClientError {
    HostClientError::new(HostClientErrorCode::InvalidEvent)
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, HostClientError> {
    serde_json::from_value(value)
        .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidEvent))
}

fn classify_host_error(status: StatusCode, bytes: &[u8]) -> HostClientError {
    let code = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| value.get("code")?.as_str().map(str::to_owned));
    let category = if code
        .as_deref()
        .is_some_and(|code| KNOWN_HOST_ERRORS.contains(&code))
    {
        HostClientErrorCode::HostFailure
    } else {
        HostClientErrorCode::UnknownHostError
    };
    HostClientError::with_status(category, status.as_u16())
}

fn find_sse_boundary(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .or_else(|| bytes.windows(4).position(|quad| quad == b"\r\n\r\n"))
}

fn sse_data(block: &[u8], max_bytes: usize) -> Result<Option<Vec<u8>>, HostClientError> {
    let text = std::str::from_utf8(block)
        .map_err(|_| HostClientError::new(HostClientErrorCode::InvalidEvent))?;
    let mut data = String::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("data: ") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value);
        }
    }
    if data.is_empty() {
        return Ok(None);
    }
    if data.len() > max_bytes {
        return Err(HostClientError::new(
            HostClientErrorCode::EventLimitExceeded,
        ));
    }
    Ok(Some(data.into_bytes()))
}
