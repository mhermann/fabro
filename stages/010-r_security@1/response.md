Review complete. I examined the full diff (single implement commit), read the surrounding code for every touched sink — vault write paths, auth extractors, demo mode, HTTP client defaults, and the compose wiring.

## Findings

### 1. Vault-controlled request destination with no scheme/host validation — SSRF surface (moderate, contingent)

**Files:** `lib/components/fabro-agent/src/web_search.rs` (`SearchBackend::SearXng`, `search_searxng`), `lib/apps/fabro-server/src/diagnostics.rs` (`check_searxng_search`)

`SEARXNG_URL` is the first vault value that becomes an outbound request **destination**. Brave and Venice hardcode their endpoints in the constructors (`BRAVE_SEARCH_URL`, `VENICE_SEARCH_URL`); only credentials flowed from the vault before. Now the vault value is spliced into `format!("{base_url}/search")` with zero validation — no scheme check, no host/IP allowlist, no private/link-local range blocking.

**What an attacker does:** Any authenticated user can write arbitrary vault entries — `POST /api/v1/secrets` (`server/handler/secrets.rs`) is gated only by `RequiredUser` (any logged-in principal, `principal_middleware.rs:390`), and `Vault::validate_name` is purely syntactic (`is_env_style_name`); the `OPTIONAL_VAULT_SECRETS` registry does not gate HTTP writes. The attacker stores `SEARXNG_URL=http://169.254.169.254/latest/meta-data` (or any internal host) and triggers any run using the agent.

**What they get:** (a) A probe oracle — status codes are returned verbatim in tool errors ("SearXNG returned status 403") and doctor summaries ("searxng: connectivity error" vs. status N), distinguishing live internal services; (b) data exfiltration — a 200 response's `results[].title/url/content` is rendered into the agent-visible tool output, so any internal endpoint that returns JSON (or can be made to) leaks its data into the run transcript; (c) the doctor probe (`/health/diagnostics`, `RequiredUser`) repeats the fetch with attacker-chosen `?q=` params. The appended `/search` path and fixed query params limit exact-target hits, but query-tolerant internal endpoints are reachable, and the base URL can carry its own path and `?`.

**Mitigating context, honestly weighed:** the product already lets any `RequiredUser` register an MCP server definition with an arbitrary `url` (`mcp_servers.rs`, schema requires `[type, url, header_keys]`) that the server process connects to, so authenticated-user-controlled outbound connections are a pre-existing accepted design property. `web_search` also requires `full` permission level, and the doctor probe is timeout-bounded. So this is an extension of an existing trust pattern rather than a brand-new hole — but the diff blesses URL-in-vault in `docs/internal/server-secrets-strategy.md` without stating the SSRF trust assumption, and adds no cheap validation (require `http(s)` scheme, optionally refuse link-local/metadata IPs). Recommend either validating or documenting the boundary explicitly.

### 2. Committed known-default SearXNG `secret_key` with limiter disabled (low)

**File:** `docker/searxng/settings.yml`

`secret_key: "fabro-local-searxng"` is a public, repo-published signing key, and `limiter: false` turns off SearXNG's bot protection. Host binding is `127.0.0.1:8888:8080` (good), so today the blast radius is the compose network and local host. But the compose file makes widening the binding a one-line edit, at which point this is an unauthenticated open search proxy with a known key — abuse vector against the operator's IP (upstream rate-limit bans) and free relay for anyone who can reach the port. The inline comment warns; acceptable for a local dev service, but this is a real committed default credential.

### 3. No request timeout on the SearXNG tool path (low)

**File:** `web_search.rs` — `search_searxng` uses `fabro_http::http_client()`, whose builder sets **no default timeout** (verified in `fabro-http/src/lib.rs`; Venice sets an explicit per-request `VENICE_REQUEST_TIMEOUT`, Brave shares the same gap). An endpoint that accepts the connection and never responds hangs the agent's `web_search` call indefinitely, stalling the workflow stage. The diagnostics probe *is* wrapped in `EXTERNAL_SERVICE_PROBE_TIMEOUT` — only the tool path is exposed. Pre-existing pattern, but the endpoint is now user-influenced via the vault.

## Checked and clean

- **No secrets committed.** `.env.example` adds an empty `SEARXNG_URL=`; the only literals are the SearXNG key above and placeholder examples (`dtn_...`, `BSA...`).
- **No injection.** Query strings go through reqwest `.query()` (percent-encoded); the doctor probe's query is constant; `base_url` is parsed as a URL by reqwest, so no header smuggling; `max_results` is clamped server-side (1–20) and enforced client-side with a `usize::try_from` fallback.
- **Redaction is correct.** `ToolSecrets`' custom `Debug` prints `searxng_search_configured: bool`, never the value — with a regression test asserting the URL is absent.
- **Demo mode is inert.** The updated doctor string in `demo/mod.rs` is a static fixture; demo `create_secret` writes nothing, and demo/real route trees are separate. No new routes were added.
- **Doctor reads vault only** (test proves env `SEARXNG_URL` is ignored server-side), matching the secrets strategy; CLI env path is operator-controlled.
- **Deserialization is safe.** Response parsed as `serde_json::Value`, only string fields extracted. Prompt injection via result text is inherent to the tool and unchanged from Brave/Venice.
- **No new crate dependencies.** The `searxng/searxng:latest` image tag is unpinned (mutable tag, supply-chain nit for a dev service), and the Docker socket mount is pre-existing and untouched.

Bottom line: no outright vulnerability class that doesn't already exist in the product's trust model, but finding 1 deserves a follow-up — a scheme/host sanity check on `SEARXNG_URL`, or at minimum an explicit SSRF trust-boundary note in the secrets strategy doc, before this pattern gets copied further.