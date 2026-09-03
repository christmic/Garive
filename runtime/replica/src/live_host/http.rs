use std::{
    collections::VecDeque, convert::Infallible, error::Error, fmt, future::Future, net::SocketAddr,
    time::Duration,
};

use axum::{
    body::{to_bytes, Body},
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream;
use serde::{de::DeserializeOwned, Deserialize};
use tokio::net::TcpListener;

use super::{
    validate_key, CancelGoalBody, CancelTurnBody, ContinueTurnBody, CreateGoalBody,
    CreateSessionBody, DispatchAgentTaskBody, ErrorBody, JoinSessionAgentBody, LiveHost,
    LiveHostError, LiveHostEvent, ReviseGoalBody, SendSessionAgentMessageBody, StartTurnBody,
    SteerTurnBody,
};
use crate::{
    CreateAgentRequest, LiveOutputReceiveError, LiveOutputSubscriber, UpdateAgentKnowledgeRequest,
};

const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// Bound loopback HTTP/SSE server for one durable [`LiveHost`].
pub struct LiveHostServer {
    listener: TcpListener,
    local_addr: SocketAddr,
    app: Router,
}

impl LiveHostServer {
    /// Binds an explicitly supplied loopback address and builds the v1 routes.
    pub async fn bind(host: LiveHost, address: SocketAddr) -> Result<Self, LiveHostServerError> {
        if !address.ip().is_loopback() {
            return Err(LiveHostServerError::NonLoopbackAddress);
        }
        let listener = TcpListener::bind(address)
            .await
            .map_err(LiveHostServerError::Io)?;
        let local_addr = listener.local_addr().map_err(LiveHostServerError::Io)?;
        let mut app = Router::new()
            .route("/v1/agents", post(create_agent).get(agent_page))
            .route("/v1/agents/:agent_id", get(agent_view).patch(update_agent))
            .route("/v1/agents/:agent_id/activate", post(activate_agent))
            .route("/v1/agents/:agent_id/archive", post(archive_agent))
            .route("/v1/agent-definitions", get(agent_definitions))
            .route("/v1/sessions", post(create_session).get(session_page))
            .route("/v1/sessions/:session_id", get(session_view))
            .route(
                "/v1/sessions/:session_id/agents",
                get(session_agents).post(join_session_agent),
            )
            .route(
                "/v1/sessions/:session_id/agents/:agent_instance_id/turns",
                post(start_agent_turn),
            )
            .route(
                "/v1/sessions/:session_id/agent-messages",
                get(session_agent_messages).post(send_session_agent_message),
            )
            .route(
                "/v1/sessions/:session_id/delegations",
                post(dispatch_agent_task),
            )
            .route(
                "/v1/sessions/:session_id/goals",
                post(create_goal).get(goal_page),
            )
            .route("/v1/sessions/:session_id/plans", get(plan_page))
            .route("/v1/sessions/:session_id/timeline", get(turn_timeline))
            .route("/v1/sessions/:session_id/turns", post(start_turn))
            .route(
                "/v1/sessions/:session_id/turns/:turn_id/steer",
                post(steer_turn_http),
            )
            .route("/v1/goals/:operation", post(mutate_goal))
            .route("/v1/turns/:operation", post(mutate_turn))
            .route("/v1/sessions/:session_id/events", get(events))
            .route(
                "/v1/management/setup",
                get(super::management::read_setup)
                    .post(super::management::commit_setup)
                    .delete(super::management::clear_setup),
            )
            .route("/v1/management/health", get(super::management::health))
            .route("/internal/mobile/wake-snapshot", get(mobile_wake_snapshot));
        if host.live_output_hub().is_some() {
            app = app.route("/v1/sessions/:session_id/live", get(live_output));
        }
        let app = app.fallback(not_found).with_state(host);
        Ok(Self {
            listener,
            local_addr,
            app,
        })
    }

    /// Returns the actual bound loopback address, including an assigned port.
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Serves until the explicit shutdown future resolves.
    pub async fn serve<F>(self, shutdown: F) -> Result<(), LiveHostServerError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(self.listener, self.app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(LiveHostServerError::Io)
    }
}

async fn live_output(State(host): State<LiveHost>, Path(session_id): Path<String>) -> Response {
    let subscriber = match host.subscribe_live_output(&session_id) {
        Ok(subscriber) => subscriber,
        Err(error) => return error_response(error),
    };
    let keepalive = Duration::from_millis(host.limits().event_poll_interval_ms);
    let events = stream::unfold(subscriber, next_live_output);
    Sse::new(events)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(keepalive)
                .text("keepalive"),
        )
        .into_response()
}

async fn next_live_output(
    mut subscriber: LiveOutputSubscriber,
) -> Option<(Result<Event, Infallible>, LiveOutputSubscriber)> {
    let value = match subscriber.recv().await {
        Ok(value) => value,
        Err(LiveOutputReceiveError::Gap | LiveOutputReceiveError::Closed) => return None,
    };
    let data = serde_json::to_string(&value).ok()?;
    let event = Event::default().event("live").data(data);
    Some((Ok(event), subscriber))
}

async fn agent_definitions(State(host): State<LiveHost>) -> Response {
    command_response(host.list_agent_definitions())
}

async fn agent_page(State(host): State<LiveHost>) -> Response {
    command_response(host.list_agents())
}

async fn agent_view(State(host): State<LiveHost>, Path(agent_id): Path<String>) -> Response {
    command_response(host.get_agent(&agent_id))
}

async fn create_agent(State(host): State<LiveHost>, headers: HeaderMap, body: Body) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let request: CreateAgentRequest = decode_body(&host, body).await?;
        host.create_agent(key, &request)
    }
    .await;
    command_response(result)
}

async fn update_agent(
    State(host): State<LiveHost>,
    Path(agent_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let request: UpdateAgentKnowledgeRequest = decode_body(&host, body).await?;
        host.update_agent_knowledge(key, &agent_id, &request)
    }
    .await;
    command_response(result)
}

async fn activate_agent(
    State(host): State<LiveHost>,
    Path(agent_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    lifecycle_agent(host, agent_id, headers, body, true).await
}

async fn archive_agent(
    State(host): State<LiveHost>,
    Path(agent_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    lifecycle_agent(host, agent_id, headers, body, false).await
}

async fn lifecycle_agent(
    host: LiveHost,
    agent_id: String,
    headers: HeaderMap,
    body: Body,
    activate: bool,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let _: EmptyBody = decode_body(&host, body).await?;
        if activate {
            host.activate_agent(key, &agent_id)
        } else {
            host.archive_agent(key, &agent_id)
        }
    }
    .await;
    command_response(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyBody {}

async fn session_view(State(host): State<LiveHost>, Path(session_id): Path<String>) -> Response {
    let result = tokio::task::spawn_blocking(move || host.get_session(&session_id))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)
        .and_then(|result| result);
    command_response(result)
}

async fn session_agents(State(host): State<LiveHost>, Path(session_id): Path<String>) -> Response {
    let result = tokio::task::spawn_blocking(move || host.get_session_agents(&session_id))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)
        .and_then(|result| result);
    command_response(result)
}

async fn session_agent_messages(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
) -> Response {
    let result = tokio::task::spawn_blocking(move || host.get_session_agent_messages(&session_id))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)
        .and_then(|result| result);
    command_response(result)
}

async fn goal_page(State(host): State<LiveHost>, Path(session_id): Path<String>) -> Response {
    let result = tokio::task::spawn_blocking(move || host.get_goals(&session_id))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)
        .and_then(|result| result);
    command_response(result)
}

async fn plan_page(State(host): State<LiveHost>, Path(session_id): Path<String>) -> Response {
    let result = tokio::task::spawn_blocking(move || host.get_plans(&session_id))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)
        .and_then(|result| result);
    command_response(result)
}

async fn session_page(State(host): State<LiveHost>, RawQuery(query): RawQuery) -> Response {
    let (limit, before) = match parse_session_query(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result = tokio::task::spawn_blocking(move || host.list_sessions(limit, before.as_deref()))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)
        .and_then(|result| result);
    command_response(result)
}

async fn mobile_wake_snapshot(State(host): State<LiveHost>, RawQuery(query): RawQuery) -> Response {
    let (limit, before) = match parse_session_query(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result =
        tokio::task::spawn_blocking(move || host.mobile_wake_page(limit, before.as_deref()))
            .await
            .map_err(|_| LiveHostError::DurabilityUnavailable)
            .and_then(|result| result);
    command_response(result)
}

async fn turn_timeline(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let (after_position, limit) = match parse_timeline_query(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let result =
        tokio::task::spawn_blocking(move || host.get_timeline(&session_id, after_position, limit))
            .await
            .map_err(|_| LiveHostError::DurabilityUnavailable)
            .and_then(|result| result);
    command_response(result)
}

/// Failure while constructing or serving the local Host listener.
#[derive(Debug)]
pub enum LiveHostServerError {
    /// H1 refuses unauthenticated non-loopback ingress.
    NonLoopbackAddress,
    /// The operating system rejected or lost the listener.
    Io(std::io::Error),
}

impl fmt::Display for LiveHostServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonLoopbackAddress => {
                formatter.write_str("live Host requires a loopback address")
            }
            Self::Io(_) => formatter.write_str("live Host listener is unavailable"),
        }
    }
}

impl Error for LiveHostServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::NonLoopbackAddress => None,
        }
    }
}

async fn create_session(State(host): State<LiveHost>, headers: HeaderMap, body: Body) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: CreateSessionBody = decode_body(&host, body).await?;
        match (
            body.agent_id.as_deref(),
            body.agent_definition_id.as_deref(),
        ) {
            (Some(agent_id), None) => host.create_registered_session(
                key,
                agent_id,
                body.agent_name.as_deref().unwrap_or(agent_id),
            ),
            (None, Some(definition_id)) => host.create_named_session(
                key,
                definition_id,
                body.agent_name.as_deref().unwrap_or(definition_id),
            ),
            _ => Err(LiveHostError::InvalidRequest),
        }
    }
    .await;
    command_response(result)
}

async fn join_session_agent(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: JoinSessionAgentBody = decode_body(&host, body).await?;
        match (
            body.agent_id.as_deref(),
            body.agent_definition_id.as_deref(),
        ) {
            (Some(agent_id), None) => {
                host.join_registered_session_agent(key, &session_id, agent_id, &body.agent_name)
            }
            (None, Some(definition_id)) => {
                host.join_session_agent(key, &session_id, definition_id, &body.agent_name)
            }
            _ => Err(LiveHostError::InvalidRequest),
        }
    }
    .await;
    command_response(result)
}

async fn send_session_agent_message(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: SendSessionAgentMessageBody = decode_body(&host, body).await?;
        host.send_session_agent_message(
            key,
            &session_id,
            &body.from_agent_instance_id,
            body.to_agent_instance_id.as_deref(),
            &body.text,
        )
    }
    .await;
    command_response(result)
}

async fn dispatch_agent_task(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: DispatchAgentTaskBody = decode_body(&host, body).await?;
        host.dispatch_agent_task(
            key,
            &session_id,
            &body.dispatcher_agent_instance_id,
            body.assignee,
            &body.delivery_policy,
            &body.objective,
        )
    }
    .await;
    match result {
        Ok(response) => {
            let background_host = host;
            let session_id = response.session_id.clone();
            let delegation_id = response.delegation_id.clone();
            tokio::spawn(async move {
                for _ in 0..6_000 {
                    let host = background_host.clone();
                    let session = session_id.clone();
                    let delegation = delegation_id.clone();
                    let delivered = tokio::task::spawn_blocking(move || {
                        host.deliver_agent_task_result(&session, &delegation)
                    })
                    .await;
                    match delivered {
                        Ok(Ok(true)) | Ok(Err(_)) | Err(_) => break,
                        Ok(Ok(false)) => tokio::time::sleep(Duration::from_millis(100)).await,
                    }
                }
            });
            command_response(Ok(response))
        }
        Err(error) => error_response(error),
    }
}

async fn create_goal(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: CreateGoalBody = decode_body(&host, body).await?;
        host.create_goal(
            key,
            &session_id,
            body.expected_session_version,
            &body.definition_json,
        )
    }
    .await;
    command_response(result)
}

async fn start_turn(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: StartTurnBody = decode_body(&host, body).await?;
        host.start_turn(key, &session_id, &body.text)
    }
    .await;
    command_response(result)
}

async fn start_agent_turn(
    State(host): State<LiveHost>,
    Path((session_id, agent_instance_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: StartTurnBody = decode_body(&host, body).await?;
        host.start_agent_turn(key, &session_id, &agent_instance_id, &body.text)
    }
    .await;
    command_response(result)
}

async fn steer_turn_http(
    State(host): State<LiveHost>,
    Path((session_id, turn_id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        let body: SteerTurnBody = decode_body(&host, body).await?;
        host.steer_turn(key, &session_id, &turn_id, &body.text)
    }
    .await;
    command_response(result)
}

async fn mutate_turn(
    State(host): State<LiveHost>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        if let Some(turn_id) = operation.strip_suffix(":cancel") {
            let body: CancelTurnBody = decode_body(&host, body).await?;
            host.cancel_turn(
                key,
                &body.session_id,
                turn_id,
                body.requested_through_position,
            )
        } else if let Some(turn_id) = operation.strip_suffix(":continue") {
            let body: ContinueTurnBody = decode_body(&host, body).await?;
            let input = match (&body.input, &body.input_json) {
                (Some(value), None) => super::HostContinuationInput::String(value),
                (None, Some(value)) => super::HostContinuationInput::Json(value),
                _ => return Err(LiveHostError::InvalidRequest),
            };
            host.continue_turn(
                key,
                &body.session_id,
                turn_id,
                &body.suspension_id,
                body.expected_session_version,
                input,
            )
        } else {
            Err(LiveHostError::InvalidRequest)
        }
    }
    .await;
    command_response(result)
}

async fn mutate_goal(
    State(host): State<LiveHost>,
    Path(operation): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let result = async {
        let key = idempotency_key(&headers)?;
        if let Some(goal_id) = operation.strip_suffix(":cancel") {
            let body: CancelGoalBody = decode_body(&host, body).await?;
            host.cancel_goal(
                key,
                &body.session_id,
                goal_id,
                body.expected_session_version,
                body.expected_revision,
                &body.reason,
            )
        } else if let Some(goal_id) = operation.strip_suffix(":revise") {
            let body: ReviseGoalBody = decode_body(&host, body).await?;
            host.revise_goal(
                key,
                &body.session_id,
                goal_id,
                body.expected_session_version,
                body.expected_revision,
                &body.definition_json,
                &body.replacement_reason,
            )
        } else {
            Err(LiveHostError::InvalidRequest)
        }
    }
    .await;
    command_response(result)
}

async fn events(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let after_position = match parse_event_query(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let first = match read_page(host.clone(), session_id.clone(), after_position).await {
        Ok(page) => page,
        Err(error) => return error_response(error),
    };
    let poll = Duration::from_millis(host.limits().event_poll_interval_ms);
    let state = EventStreamState {
        host,
        session_id,
        cursor: first.scanned_through_position,
        pending: first.events.into(),
        poll,
    };
    let events = stream::unfold(state, next_event);
    Sse::new(events)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(poll)
                .text("keepalive"),
        )
        .into_response()
}

struct EventStreamState {
    host: LiveHost,
    session_id: String,
    cursor: u64,
    pending: VecDeque<LiveHostEvent>,
    poll: Duration,
}

async fn next_event(
    mut state: EventStreamState,
) -> Option<(Result<Event, Infallible>, EventStreamState)> {
    loop {
        if let Some(value) = state.pending.pop_front() {
            let data = match serde_json::to_string(&value) {
                Ok(data) => data,
                Err(_) => return None,
            };
            let event = Event::default()
                .id(value.position.to_string())
                .event("host")
                .data(data);
            return Some((Ok(event), state));
        }
        match read_page(state.host.clone(), state.session_id.clone(), state.cursor).await {
            Ok(page) => {
                state.cursor = page.scanned_through_position;
                state.pending = page.events.into();
                if state.pending.is_empty() {
                    tokio::time::sleep(state.poll).await;
                }
            }
            Err(_) => return None,
        }
    }
}

async fn read_page(
    host: LiveHost,
    session_id: String,
    after_position: u64,
) -> Result<super::HostEventPage, LiveHostError> {
    tokio::task::spawn_blocking(move || host.read_event_page(&session_id, after_position))
        .await
        .map_err(|_| LiveHostError::DurabilityUnavailable)?
}

pub(crate) async fn decode_body<T: DeserializeOwned>(
    host: &LiveHost,
    body: Body,
) -> Result<T, LiveHostError> {
    let bytes = to_bytes(body, host.limits().max_command_bytes)
        .await
        .map_err(|_| LiveHostError::InvalidRequest)?;
    serde_json::from_slice(&bytes).map_err(|_| LiveHostError::InvalidRequest)
}

fn idempotency_key(headers: &HeaderMap) -> Result<&str, LiveHostError> {
    let mut values = headers.get_all(IDEMPOTENCY_KEY).iter();
    let value = values.next().ok_or(LiveHostError::InvalidRequest)?;
    if values.next().is_some() {
        return Err(LiveHostError::InvalidRequest);
    }
    let value = value.to_str().map_err(|_| LiveHostError::InvalidRequest)?;
    validate_key(value)?;
    Ok(value)
}

fn parse_event_query(query: Option<&str>) -> Result<u64, LiveHostError> {
    match query {
        None | Some("") => Ok(0),
        Some(value) => {
            let mut pairs = value.split('&');
            let pair = pairs.next().ok_or(LiveHostError::InvalidRequest)?;
            if pairs.next().is_some() {
                return Err(LiveHostError::InvalidRequest);
            }
            let raw = pair
                .strip_prefix("after_position=")
                .ok_or(LiveHostError::InvalidRequest)?;
            raw.parse().map_err(|_| LiveHostError::InvalidRequest)
        }
    }
}

fn parse_session_query(query: Option<&str>) -> Result<(usize, Option<String>), LiveHostError> {
    let query = query
        .filter(|value| !value.is_empty())
        .ok_or(LiveHostError::InvalidRequest)?;
    let mut limit = None;
    let mut before = None;
    for pair in query.split('&') {
        if let Some(raw) = pair.strip_prefix("limit=") {
            if limit.is_some() || raw.is_empty() {
                return Err(LiveHostError::InvalidRequest);
            }
            limit = Some(raw.parse().map_err(|_| LiveHostError::InvalidRequest)?);
        } else if let Some(raw) = pair.strip_prefix("before=") {
            if before.is_some()
                || raw.is_empty()
                || !raw
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err(LiveHostError::InvalidRequest);
            }
            before = Some(raw.to_owned());
        } else {
            return Err(LiveHostError::InvalidRequest);
        }
    }
    Ok((limit.ok_or(LiveHostError::InvalidRequest)?, before))
}

fn parse_timeline_query(query: Option<&str>) -> Result<(u64, usize), LiveHostError> {
    let query = query
        .filter(|value| !value.is_empty())
        .ok_or(LiveHostError::InvalidRequest)?;
    let mut after_position = None;
    let mut limit = None;
    for pair in query.split('&') {
        if let Some(raw) = pair.strip_prefix("after_position=") {
            if after_position.is_some() || raw.is_empty() {
                return Err(LiveHostError::InvalidRequest);
            }
            after_position = Some(raw.parse().map_err(|_| LiveHostError::InvalidRequest)?);
        } else if let Some(raw) = pair.strip_prefix("limit=") {
            if limit.is_some() || raw.is_empty() {
                return Err(LiveHostError::InvalidRequest);
            }
            limit = Some(raw.parse().map_err(|_| LiveHostError::InvalidRequest)?);
        } else {
            return Err(LiveHostError::InvalidRequest);
        }
    }
    Ok((
        after_position.ok_or(LiveHostError::InvalidRequest)?,
        limit.ok_or(LiveHostError::InvalidRequest)?,
    ))
}

fn command_response<T: serde::Serialize>(result: Result<T, LiveHostError>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => error_response(error),
    }
}

fn error_response(error: LiveHostError) -> Response {
    let status = match error {
        LiveHostError::InvalidRequest => StatusCode::BAD_REQUEST,
        LiveHostError::NotFound => StatusCode::NOT_FOUND,
        LiveHostError::CommandConflict | LiveHostError::ConcurrentModification => {
            StatusCode::CONFLICT
        }
        LiveHostError::SessionBusy => StatusCode::CONFLICT,
        LiveHostError::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
        LiveHostError::DurabilityUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        LiveHostError::ReadBoundExceeded => StatusCode::PAYLOAD_TOO_LARGE,
        LiveHostError::CorruptState => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ErrorBody {
            code: error.code(),
            message: error.message(),
        }),
    )
        .into_response()
}

async fn not_found() -> Response {
    error_response(LiveHostError::NotFound)
}
