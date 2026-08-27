# engine/

Agent core. Primary implementation is **Rust** (Cargo workspace).
The Kotlin mirror in `experiments/kotlin/` tracks this tree
semantically.

Sub-directories land as their slices are scoped; planned members
are listed in the root `Cargo.toml`.

## Sub-directories

| Path | Role |
|------|------|
| `core/` | Agent loop, runtime primitives, contracts. |
| `ledger/` | Durable, append-only event log (decisions, actions, outcomes). |
| `1lm/` | Language-model abstraction (provider-agnostic). |
| `tools/` | Tool registry and execution surface. |
| `memory/` | Short- and long-term memory layers. |
| `knowledge/` | Knowledge store and retrieval. |
| `skill/` | Skill packaging, loading, execution. |
| `multiagent/` | Coordination primitives for multi-agent runs. |
| `scheduler/` | Task and turn scheduling. |
| `creativity/` | Value discovery, ideation, exploration. |
| `eval/` | Evaluation harness for agent behaviour. |
| `observability/` | Tracing, metrics, structured logs. |
| `config/` | Configuration schema and loaders. |
| `proto/` | Generated protobuf wire types (single source: `spec/proto/`). |

Each sub-directory starts empty and is populated when its slice
lands. Adding a new engine crate = create the sub-dir + its
`Cargo.toml` + register it in the root workspace `members`.