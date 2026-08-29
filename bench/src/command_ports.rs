use std::{collections::BTreeSet, process::Stdio, time::Duration};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    AgentDriver, AgentInput, AgentOutput, BenchError, BenchErrorCode, BenchFuture, EnvironmentPool,
    SweCase, WorkspaceLease,
};

/// Explicit subprocess configuration for an injected benchmark port.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommandPortConfig {
    /// Executable path or admitted command name.
    pub executable: String,
    /// Exact fixed argv before the operation name.
    pub arguments: Vec<String>,
    /// Explicit working directory.
    pub working_directory: String,
    /// Explicit environment after inherited values are cleared.
    pub environment: Vec<(String, String)>,
    /// Complete operation timeout.
    pub timeout_ms: u64,
    /// Maximum stdout response bytes.
    pub max_output_bytes: usize,
}

impl CommandPortConfig {
    fn validate(&self) -> Result<(), BenchError> {
        let keys = self
            .environment
            .iter()
            .map(|(key, _)| key)
            .collect::<BTreeSet<_>>();
        if self.executable.is_empty()
            || self.working_directory.is_empty()
            || self.timeout_ms == 0
            || self.max_output_bytes == 0
            || self.arguments.iter().any(|value| value.contains('\0'))
            || keys.len() != self.environment.len()
            || self
                .environment
                .iter()
                .any(|(key, value)| key.is_empty() || key.contains('=') || value.contains('\0'))
        {
            Err(infrastructure())
        } else {
            Ok(())
        }
    }
}

/// Command-backed warm environment broker.
pub struct CommandEnvironmentPool {
    config: CommandPortConfig,
    warm_capacity: usize,
}

impl CommandEnvironmentPool {
    /// Creates a broker with explicit non-zero warm capacity.
    pub fn new(config: CommandPortConfig, warm_capacity: usize) -> Result<Self, BenchError> {
        config.validate()?;
        if warm_capacity == 0 || warm_capacity > 64 {
            return Err(infrastructure());
        }
        Ok(Self {
            config,
            warm_capacity,
        })
    }
}

impl EnvironmentPool for CommandEnvironmentPool {
    fn warm_capacity(&self) -> usize {
        self.warm_capacity
    }

    fn acquire<'a>(&'a self, case: &'a SweCase) -> BenchFuture<'a, WorkspaceLease> {
        Box::pin(async move {
            let response: AcquireResponse = invoke_json(
                &self.config,
                "acquire",
                &AcquireRequest {
                    instance_id: case.instance_id.as_str(),
                    repository: &case.repository,
                    base_commit: &case.base_commit,
                },
            )
            .await?;
            Ok(WorkspaceLease {
                handle: response.handle,
                case_id: response.case_id,
                base_commit: response.base_commit,
            })
        })
    }

    fn release<'a>(&'a self, lease: WorkspaceLease) -> BenchFuture<'a, ()> {
        Box::pin(async move {
            let response: ReleaseResponse = invoke_json(
                &self.config,
                "release",
                &ReleaseRequest {
                    handle: &lease.handle,
                    case_id: &lease.case_id,
                    base_commit: &lease.base_commit,
                },
            )
            .await?;
            if response.released {
                Ok(())
            } else {
                Err(infrastructure())
            }
        })
    }
}

/// Command-backed injected Agent driver.
pub struct CommandAgentDriver {
    config: CommandPortConfig,
}

impl CommandAgentDriver {
    /// Creates an Agent driver with no environment/config discovery.
    pub fn new(config: CommandPortConfig) -> Result<Self, BenchError> {
        config.validate()?;
        Ok(Self { config })
    }
}

impl AgentDriver for CommandAgentDriver {
    fn run<'a>(
        &'a self,
        input: AgentInput,
        workspace: &'a WorkspaceLease,
    ) -> BenchFuture<'a, AgentOutput> {
        Box::pin(async move {
            let response: AgentResponse = invoke_json(
                &self.config,
                "run",
                &AgentRequest {
                    payload: &input.payload,
                    repository: &input.repository,
                    base_commit: &input.base_commit,
                    workspace_handle: &workspace.handle,
                },
            )
            .await?;
            if response.raw.is_empty() {
                return Err(infrastructure());
            }
            Ok(AgentOutput {
                raw: response.raw,
                duration_ms: response.duration_ms,
                input_tokens: response.input_tokens,
                output_tokens: response.output_tokens,
            })
        })
    }
}

#[derive(Serialize)]
struct AcquireRequest<'a> {
    instance_id: &'a str,
    repository: &'a str,
    base_commit: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcquireResponse {
    handle: String,
    case_id: String,
    base_commit: String,
}

#[derive(Serialize)]
struct ReleaseRequest<'a> {
    handle: &'a str,
    case_id: &'a str,
    base_commit: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseResponse {
    released: bool,
}

#[derive(Serialize)]
struct AgentRequest<'a> {
    payload: &'a str,
    repository: &'a str,
    base_commit: &'a str,
    workspace_handle: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentResponse {
    raw: String,
    duration_ms: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

async fn invoke_json<T: Serialize, R: DeserializeOwned>(
    config: &CommandPortConfig,
    operation: &str,
    request: &T,
) -> Result<R, BenchError> {
    let input = serde_json::to_vec(request).map_err(|_| infrastructure())?;
    let future = async {
        let mut child = tokio::process::Command::new(&config.executable)
            .args(&config.arguments)
            .arg(operation)
            .current_dir(&config.working_directory)
            .env_clear()
            .envs(config.environment.clone())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| infrastructure())?;
        let mut stdin = child.stdin.take().ok_or_else(infrastructure)?;
        stdin
            .write_all(&input)
            .await
            .map_err(|_| infrastructure())?;
        stdin.shutdown().await.map_err(|_| infrastructure())?;
        drop(stdin);
        let stdout = child.stdout.take().ok_or_else(infrastructure)?;
        let mut output = Vec::new();
        stdout
            .take(config.max_output_bytes as u64 + 1)
            .read_to_end(&mut output)
            .await
            .map_err(|_| infrastructure())?;
        if output.len() > config.max_output_bytes {
            child.kill().await.map_err(|_| infrastructure())?;
            return Err(infrastructure());
        }
        let status = child.wait().await.map_err(|_| infrastructure())?;
        if !status.success() {
            return Err(infrastructure());
        }
        serde_json::from_slice(&output).map_err(|_| infrastructure())
    };
    tokio::time::timeout(Duration::from_millis(config.timeout_ms), future)
        .await
        .map_err(|_| infrastructure())?
}

fn infrastructure() -> BenchError {
    BenchError::from_port(BenchErrorCode::InfrastructureFailure)
}
