//! Direct bounded WebSocket transport for one managed Chromium endpoint.

use std::{collections::VecDeque, error::Error, fmt, time::Duration};

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};

use crate::{
    parse_incoming, CdpAdapterConfig, CdpCommand, CdpError, CdpIncoming, CdpLimits,
    CdpProtocolError,
};

const MAX_JS_UINT: u64 = 9_007_199_254_740_991;

/// One connected, sequential and strictly correlated managed-browser transport.
pub struct CdpTransport {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    limits: CdpLimits,
    next_id: u64,
    events: VecDeque<CdpIncoming>,
}

impl CdpTransport {
    /// Connects directly to the explicit loopback endpoint with frame limits applied by Tungstenite.
    pub async fn connect(config: &CdpAdapterConfig) -> Result<Self, CdpTransportError> {
        let limits = config.limits();
        let websocket = WebSocketConfig::default()
            .max_message_size(Some(limits.max_frame_bytes))
            .max_frame_size(Some(limits.max_frame_bytes));
        let connect = connect_async_with_config(config.endpoint().as_str(), Some(websocket), false);
        let (stream, _) =
            tokio::time::timeout(Duration::from_millis(limits.operation_timeout_ms), connect)
                .await
                .map_err(|_| CdpTransportError::Timeout)?
                .map_err(|_| CdpTransportError::ConnectFailed)?;
        Ok(Self {
            stream,
            limits,
            next_id: 1,
            events: VecDeque::new(),
        })
    }

    /// Sends one admitted command and waits for its exact response while bounding events.
    pub async fn call(
        &mut self,
        method: impl Into<String>,
        params: Value,
        session_id: Option<String>,
    ) -> Result<Value, CdpTransportError> {
        let duration = Duration::from_millis(self.limits.operation_timeout_ms);
        tokio::time::timeout(duration, self.call_inner(method.into(), params, session_id))
            .await
            .map_err(|_| CdpTransportError::Timeout)?
    }

    /// Removes the oldest bounded unsolicited event, if any.
    pub fn pop_event(&mut self) -> Option<CdpIncoming> {
        self.events.pop_front()
    }

    /// Waits for one exact event method/session while retaining bounded unrelated events.
    pub async fn wait_for_event(
        &mut self,
        method: &str,
        session_id: Option<&str>,
    ) -> Result<Value, CdpTransportError> {
        self.wait_for_event_matching(method, session_id, |_| true)
            .await
    }

    /// Waits for an exact event whose bounded parameters satisfy a pure predicate.
    pub async fn wait_for_event_matching<F>(
        &mut self,
        method: &str,
        session_id: Option<&str>,
        mut predicate: F,
    ) -> Result<Value, CdpTransportError>
    where
        F: FnMut(&Value) -> bool,
    {
        if method.is_empty() || session_id == Some("") {
            return Err(CdpTransportError::Protocol(
                CdpProtocolError::InvalidMessage,
            ));
        }
        if let Some(index) = self.events.iter().position(|event| {
            matches!(event, CdpIncoming::Event { method: queued, params, session_id: queued_session }
                if queued == method && queued_session.as_deref() == session_id && predicate(params))
        }) {
            let Some(CdpIncoming::Event { params, .. }) = self.events.remove(index) else {
                return Err(CdpTransportError::Protocol(
                    CdpProtocolError::InvalidMessage,
                ));
            };
            return Ok(params);
        }
        let duration = Duration::from_millis(self.limits.operation_timeout_ms);
        tokio::time::timeout(
            duration,
            self.wait_for_event_inner(method, session_id.map(str::to_owned), &mut predicate),
        )
        .await
        .map_err(|_| CdpTransportError::Timeout)?
    }

    async fn call_inner(
        &mut self,
        method: String,
        params: Value,
        session_id: Option<String>,
    ) -> Result<Value, CdpTransportError> {
        let id = self.allocate_id()?;
        let command = CdpCommand::new(id, method, params, session_id.clone())
            .map_err(CdpTransportError::Protocol)?;
        let text = serde_json::to_string(&command).map_err(|_| CdpTransportError::EncodeFailed)?;
        if text.len() > self.limits.max_frame_bytes {
            return Err(CdpTransportError::Protocol(
                CdpProtocolError::FrameBoundExceeded,
            ));
        }
        self.stream
            .send(Message::Text(text.into()))
            .await
            .map_err(|_| CdpTransportError::ConnectionLost)?;
        loop {
            let message = self
                .stream
                .next()
                .await
                .ok_or(CdpTransportError::ConnectionLost)?
                .map_err(|_| CdpTransportError::ConnectionLost)?;
            let incoming = match message {
                Message::Text(text) => parse_incoming(text.as_bytes(), self.limits.max_frame_bytes)
                    .map_err(CdpTransportError::Protocol)?,
                Message::Ping(payload) => {
                    self.stream
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|_| CdpTransportError::ConnectionLost)?;
                    continue;
                }
                Message::Pong(_) => continue,
                Message::Close(_) => return Err(CdpTransportError::ConnectionLost),
                Message::Binary(_) | Message::Frame(_) => {
                    return Err(CdpTransportError::UnsupportedFrame)
                }
            };
            match incoming {
                CdpIncoming::Result {
                    id: response_id,
                    result,
                    session_id: response_session,
                } if response_id == id && response_session == session_id => return Ok(result),
                CdpIncoming::Error {
                    id: response_id,
                    error,
                    session_id: response_session,
                } if response_id == id && response_session == session_id => {
                    return Err(CdpTransportError::Remote(error))
                }
                event @ CdpIncoming::Event { .. } => {
                    if self.events.len() >= self.limits.max_queued_events {
                        return Err(CdpTransportError::EventQueueExceeded);
                    }
                    self.events.push_back(event);
                }
                _ => return Err(CdpTransportError::CorrelationMismatch),
            }
        }
    }

    async fn wait_for_event_inner<F>(
        &mut self,
        method: &str,
        session_id: Option<String>,
        predicate: &mut F,
    ) -> Result<Value, CdpTransportError>
    where
        F: FnMut(&Value) -> bool,
    {
        loop {
            let incoming = self.read_incoming().await?;
            match incoming {
                CdpIncoming::Event {
                    method: incoming_method,
                    params,
                    session_id: incoming_session,
                } if incoming_method == method
                    && incoming_session == session_id
                    && predicate(&params) =>
                {
                    return Ok(params)
                }
                event @ CdpIncoming::Event { .. } => {
                    if self.events.len() >= self.limits.max_queued_events {
                        return Err(CdpTransportError::EventQueueExceeded);
                    }
                    self.events.push_back(event);
                }
                _ => return Err(CdpTransportError::CorrelationMismatch),
            }
        }
    }

    async fn read_incoming(&mut self) -> Result<CdpIncoming, CdpTransportError> {
        loop {
            let message = self
                .stream
                .next()
                .await
                .ok_or(CdpTransportError::ConnectionLost)?
                .map_err(|_| CdpTransportError::ConnectionLost)?;
            match message {
                Message::Text(text) => {
                    return parse_incoming(text.as_bytes(), self.limits.max_frame_bytes)
                        .map_err(CdpTransportError::Protocol)
                }
                Message::Ping(payload) => {
                    self.stream
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|_| CdpTransportError::ConnectionLost)?;
                }
                Message::Pong(_) => {}
                Message::Close(_) => return Err(CdpTransportError::ConnectionLost),
                Message::Binary(_) | Message::Frame(_) => {
                    return Err(CdpTransportError::UnsupportedFrame)
                }
            }
        }
    }

    fn allocate_id(&mut self) -> Result<u64, CdpTransportError> {
        if self.next_id == 0 || self.next_id > MAX_JS_UINT {
            return Err(CdpTransportError::CorrelationExhausted);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        Ok(id)
    }
}

/// Stable transport failure without leaking endpoint or browser content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CdpTransportError {
    /// Direct loopback WebSocket connection failed.
    ConnectFailed,
    /// Command could not be encoded.
    EncodeFailed,
    /// Connection closed or failed during an exchange.
    ConnectionLost,
    /// Command exceeded its explicit wall-clock limit.
    Timeout,
    /// Browser reported navigation did not start or commit successfully.
    NavigationFailed,
    /// Incoming or outgoing protocol data failed validation.
    Protocol(CdpProtocolError),
    /// Browser returned one correlated protocol error.
    Remote(CdpError),
    /// Response did not match the active command and flat session.
    CorrelationMismatch,
    /// JavaScript-safe correlation identities were exhausted.
    CorrelationExhausted,
    /// Unsolicited event queue reached its hard bound.
    EventQueueExceeded,
    /// CDP sent a non-text application frame.
    UnsupportedFrame,
}

impl fmt::Display for CdpTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ConnectFailed => "CDP connection failed",
            Self::EncodeFailed => "CDP command encoding failed",
            Self::ConnectionLost => "CDP connection lost",
            Self::Timeout => "CDP command timed out",
            Self::NavigationFailed => "CDP navigation failed",
            Self::Protocol(_) => "CDP protocol validation failed",
            Self::Remote(_) => "CDP remote command failed",
            Self::CorrelationMismatch => "CDP response correlation mismatch",
            Self::CorrelationExhausted => "CDP correlation identity exhausted",
            Self::EventQueueExceeded => "CDP event queue bound exceeded",
            Self::UnsupportedFrame => "CDP unsupported WebSocket frame",
        })
    }
}

impl Error for CdpTransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}
