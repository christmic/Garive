# Garive Web

Garive Web mounts the same React Work UI and pure product controller as the
macOS client. Its composition port uses strict H1/H2 HTTP/SSE and bounded
browser preferences; it does not own Agent or durable Session truth.

## Run locally

Start a local Runtime Host on `127.0.0.1:8787`, then:

```sh
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

The `?visual-test=...` routes are development-only deterministic presentation
fixtures. They are suitable for shared UI screenshots, never live Runtime or
filesystem claims.
