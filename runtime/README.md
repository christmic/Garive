# runtime/

Service-runtime tier: containers and gateways that host the agent
core.

## Sub-directories

| Path | Language | Role |
|------|----------|------|
| `replica/` | Rust | The replica — the service container that runs an Agent process. |
| `gateway/` | Go | High-throughput, stable gateway (auth, rate limit, load balance, observability, routing). |

The replica embeds the Rust engine crates and exposes an interface the
gateway talks to. The gateway is an independent Go module
(`go.mod`), not part of the Rust workspace.