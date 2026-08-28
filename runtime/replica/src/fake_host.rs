use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostEventKind {
    SessionCreated,
    TurnStarted,
    OutputDelta,
    TurnCompleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostEvent {
    pub position: u64,
    pub kind: HostEventKind,
    pub text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FakeHost {
    expected_input: String,
    events: Vec<HostEvent>,
}

impl FakeHost {
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

    pub fn run(&self, input: &str) -> Result<&[HostEvent], &'static str> {
        if input != self.expected_input {
            return Err("fake host accepts only fixture input");
        }
        Ok(&self.events)
    }
}
