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
use serde::de::DeserializeOwned;
use tokio::net::TcpListener;

use super::{
    validate_key, AgentDefinitionPage, CancelTurnBody, ContinueTurnBody, CreateSessionBody,
    ErrorBody, LiveHost, LiveHostError, LiveHostEvent, SessionPage, StartTurnBody,
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
        let app = Router::new()
            .route("/v1/agent-definitions", get(agent_definitions))
            .route("/v1/sessions", get(list_sessions).post(create_session))
            .route("/v1/sessions/:session_id", get(get_session))
            .route("/v1/sessions/:session_id/turns", post(start_turn))
            .route("/v1/sessions/:session_id/timeline", get(timeline))
            .route("/v1/turns/:operation", post(mutate_turn))
            .route("/v1/sessions/:session_id/events", get(events))
            .route("/internal/mobile/wake-snapshot", get(mobile_wake_snapshot))
            .fallback(not_found)
            .with_state(host);
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

async fn agent_definitions(State(host): State<LiveHost>, RawQuery(query): RawQuery) -> Response {
    if query.is_some_and(|value| !value.is_empty()) {
        return error_response(LiveHostError::InvalidRequest);
    }
    command_response(Ok(AgentDefinitionPage {
        api_version: "v1",
        definitions: host.agent_definitions(),
    }))
}

async fn list_sessions(State(host): State<LiveHost>, RawQuery(query): RawQuery) -> Response {
    let limit = match parse_single_limit(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    command_response(host.list_sessions(limit).map(|sessions| SessionPage {
        api_version: "v1",
        sessions,
        next_before: None,
    }))
}

async fn get_session(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    if query.is_some_and(|value| !value.is_empty()) {
        return error_response(LiveHostError::InvalidRequest);
    }
    command_response(host.get_session(&session_id))
}

async fn mobile_wake_snapshot(State(host): State<LiveHost>, RawQuery(query): RawQuery) -> Response {
    let (limit, before) = match parse_wake_query(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    command_response(host.mobile_wake_page(limit, before.as_deref()))
}

async fn timeline(
    State(host): State<LiveHost>,
    Path(session_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let (after_position, limit) = match parse_timeline_query(query.as_deref()) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    command_response(host.read_timeline(&session_id, after_position, limit))
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
        host.create_session(key, &body.agent_definition_id)
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

async fn decode_body<T: DeserializeOwned>(host: &LiveHost, body: Body) -> Result<T, LiveHostError> {
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

fn parse_single_limit(query: Option<&str>) -> Result<usize, LiveHostError> {
    let value = query.ok_or(LiveHostError::InvalidRequest)?;
    let raw = value
        .strip_prefix("limit=")
        .ok_or(LiveHostError::InvalidRequest)?;
    if raw.is_empty() || raw.contains('&') {
        return Err(LiveHostError::InvalidRequest);
    }
    raw.parse().map_err(|_| LiveHostError::InvalidRequest)
}

fn parse_timeline_query(query: Option<&str>) -> Result<(u64, usize), LiveHostError> {
    let mut after_position = None;
    let mut limit = None;
    for pair in query.ok_or(LiveHostError::InvalidRequest)?.split('&') {
        let (key, value) = pair.split_once('=').ok_or(LiveHostError::InvalidRequest)?;
        match key {
            "after_position" if after_position.is_none() => {
                after_position = Some(value.parse().map_err(|_| LiveHostError::InvalidRequest)?);
            }
            "limit" if limit.is_none() => {
                limit = Some(value.parse().map_err(|_| LiveHostError::InvalidRequest)?);
            }
            _ => return Err(LiveHostError::InvalidRequest),
        }
    }
    Ok((
        after_position.ok_or(LiveHostError::InvalidRequest)?,
        limit.ok_or(LiveHostError::InvalidRequest)?,
    ))
}

fn parse_wake_query(query: Option<&str>) -> Result<(usize, Option<String>), LiveHostError> {
    let mut limit = None;
    let mut before = None;
    for pair in query.ok_or(LiveHostError::InvalidRequest)?.split('&') {
        let (key, value) = pair.split_once('=').ok_or(LiveHostError::InvalidRequest)?;
        match key {
            "limit" if limit.is_none() => {
                limit = Some(value.parse().map_err(|_| LiveHostError::InvalidRequest)?);
            }
            "before" if before.is_none() && !value.is_empty() => {
                validate_key(value)?;
                before = Some(value.to_owned());
            }
            _ => return Err(LiveHostError::InvalidRequest),
        }
    }
    Ok((limit.ok_or(LiveHostError::InvalidRequest)?, before))
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
        LiveHostError::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
        LiveHostError::DurabilityUnavailable => StatusCode::SERVICE_UNAVAILABLE,
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
