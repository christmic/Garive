# Garive Web

Garive Web mounts the same React Work UI and pure product controller as the
macOS client. Its composition port uses strict H1/H2 HTTP/SSE and bounded
browser preferences; it does not own Agent or durable Session truth.

## Run locally

Configure and start the shipping loopback Host, then start Web:

```sh
printf '%s\n' "$GARIVE_CONNECTION_CREDENTIAL" | cargo run -p garive-desktop \
  --bin garive-host -- configure "$GARIVE_CONFIG_DIR" anthropic.messages.v1 \
  http://127.0.0.1:9527/v1/messages token9-deepseek deepseek-v4-flash garive-work
cargo run -p garive-desktop --bin garive-host -- serve "$GARIVE_CONFIG_DIR"
```

For a locked/headless Mac or a local gateway that injects the upstream secret,
the Host can read a write-only credential from stdin instead of opening the OS
credential store:

```sh
printf '%s\n' "$GARIVE_CONNECTION_CREDENTIAL" | cargo run -p garive-desktop \
  --bin garive-host -- serve-stdin "$GARIVE_CONFIG_DIR"

cd web
pnpm install --frozen-lockfile
pnpm dev
```

Open `http://127.0.0.1:1430/`. Vite proxies `/v1` to the loopback Host so the
browser uses a same-origin connection rather than wildcard CORS. A deployment
must provide the equivalent same-origin `/v1` reverse proxy. For an explicit
loopback composition, pass `?host=http://127.0.0.1:PORT/`; that Host must admit
the exact browser origin.

`pnpm test` covers the strict transport, shared projection boundary, and local
preference adapter. `pnpm build` produces the shipping static bundle.

Set `GARIVE_LIVE_HOST_URL`, `GARIVE_LIVE_DEFINITION_ID`, and
`GARIVE_LIVE_EXPECTED_TEXT` to include the opt-in real-model Web transport test.

The `?visual-test=...` routes are development-only deterministic presentation
fixtures. They are suitable for shared UI screenshots, never live Runtime or
filesystem claims.
