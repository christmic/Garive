use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Small admitted event vocabulary emitted by the fixture-backed shell host.
pub enum HostEventKind {
    /// A new fixture session became visible.
    SessionCreated,
    /// The fixture turn started.
    TurnStarted,
    /// One ordered response fragment became visible.
    OutputDelta,
    /// The fixture turn reached its sole terminal event.
    TurnCompleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Ordered event exposed by [`FakeHost`] for shell integration only.
pub struct HostEvent {
    /// One-based contiguous event position.
    pub position: u64,
    /// Semantic event class.
    pub kind: HostEventKind,
    /// Optional text carried by output events.
    pub text: Option<String>,
}

#[derive(Clone, Debug)]
/// Deterministic fixture-backed host used until the durable live Host exists.
pub struct FakeHost {
    expected_input: String,
    events: Vec<HostEvent>,
}

impl FakeHost {
    /// Parses and validates one `garive.host.v1` fake-session fixture.
    pub fn from_fixture(bytes: &[u8]) -> Result<Self, &'static str> {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| "invalid fixture json")?;
        if value["api_version"] != "garive.host.v1" {
            return Err("unsupported host api");
        }
        let expected_input = value["command"]["text"]
            .as_str()
            .ok_or("missing command text")?
            .to_owned();
        let values = value["events"].as_array().ok_or("missing events")?;
        let mut events = Vec::with_capacity(values.len());
        let mut previous = 0;
        let mut terminal = false;
        for value in values {
            let position = value["position"].as_u64().ok_or("invalid position")?;
            if position != previous + 1 || terminal {
                return Err("invalid event order");
            }
            previous = position;
            let kind = match value["event"].as_str().ok_or("missing event kind")? {
                "session.created" => HostEventKind::SessionCreated,
                "turn.started" => HostEventKind::TurnStarted,
                "output.delta" => HostEventKind::OutputDelta,
                "turn.completed" => {
                    terminal = true;
                    HostEventKind::TurnCompleted
                }
                _ => return Err("unsupported event kind"),
            };
            events.push(HostEvent {
                position,
                kind,
                text: value["text"].as_str().map(str::to_owned),
            });
        }
        if !terminal {
            return Err("missing terminal");
        }
        Ok(Self {
            expected_input,
            events,
        })
    }

    /// Returns the frozen event stream when `input` matches the fixture command.
    pub fn run(&self, input: &str) -> Result<&[HostEvent], &'static str> {
        if input != self.expected_input {
            return Err("fake host accepts only fixture input");
        }
        Ok(&self.events)
    }
}
