Review complete. I read the full diff, the complete files behind it, the auth/vault trust model, and the reqwest source backing the HTTP error paths. No files were modified.

## What the change is

A third `SearchBackend::Searxng` variant for the `web_search` tool, selected first when vault/env `SEARXNG_URL` is present, plus diagnostics probing, doc pages, and tests. The tool schema, permissions, and event surfaces are unchanged.

## Findings

### 1. Broken redaction invariant on the error path — `#[expect]` justification is factually false (Low–Medium, credential exposure)

`lib/components/fabro-agent/src/web_search.rs:204-221` suppresses the `disallowed_types` lint with the reason *"the URL is never logged or rendered."* That is not true. The raw URL **is** rendered on routine failure paths:

```rust
.map_err(|e| format!("HTTP request failed: {e}"))?;
```

reqwest 0.12.28 attaches the request URL to every send/timeout error via `.with_url(self.url.clone())` (`async_impl/client.rs:3048, 3056, 3071`) and `Error::Display` prints it **unredacted**: ` for url ({url})` (`error.rs:267-268`). reqwest's own docs at `error.rs:16` state the caller must remove it (`without_url`). Note the timeout case — `SEARXNG_REQUEST_TIMEOUT` errors carry the URL too, and timeouts are the *expected* failure mode for a slow aggregating instance.

**What happens:** if the operator configures `SEARXNG_URL=http://user:password@searxng:8080` (basic auth is a natural way to protect a SearXNG instance), the password ends up in the returned error string → agent-visible tool output → session events/SSE/run logs. The same URL text flows into `lib/apps/fabro-server/src/diagnostics.rs` via `CheckDetail::new(format!("{err:#}"))`, which any authenticated user can retrieve through `POST /api/v1/health/diagnostics`.

This is exactly the boundary `clippy.toml` encodes ("Use fabro_redact::DisplaySafeUrl at logging/error boundaries for URLs that may carry credentials"), and the repo already ships `fabro_redact::DisplaySafeUrl` for it. The Bedrock precedent the comment mimics is materially different: there the error is fixed text that never contains the URL, so its justification is true; here it is not. Fix: redact the error (`e.without_url()` or render via `DisplaySafeUrl`), or strip userinfo from the URL before requesting.

### 2. New vault-controlled egress destination — authenticated-user SSRF surface (Medium in multi-user deployments; accepted-risk in single-tenant)

Pre-diff, every vault-settable value was a **credential for a hardcoded destination** (Brave/Venice URLs are constants; `DAYTONA_API_URL` — the analogous URL config — is deliberately operator-env-only, never vault). This diff makes the vault select the **destination**: any authenticated user can call `POST /api/v1/secrets` with `{"name": "SEARXNG_URL", "value": "http://169.254.169.254:8080/"}` — the handler (`server/handler/secrets.rs:27`) rejects only bootstrap names and there is no admin/role model anywhere in the auth layer — after which the **server process** (not the run sandbox; the executor calls `fabro_http` directly) issues `GET attacker-chosen-host:port/search?q=…&format=json`.

**What the attacker gets:**
- A clean network-probe oracle from the server's privileged network position: connectivity-error vs. `searxng: HTTP <status>` vs. parse-failure vs. timeout — readable both from their own agent sessions and from `POST /health/diagnostics`, no run needed.
- Partial content exfiltration for any JSON body shaped like `results[].{title,url,content}` (internal APIs with that shape, or lucky metadata endpoints).

Validation is scheme-only (`http|https`); there is no loopback/link-local/metadata-host policy, and the diagnostics probe (`check_searxng_search`) has no URL validation at all. The probe path is pinned to `/search`, which bounds the read primitive. In the documented single-tenant trusted deployment this is arguably consented operator config; in any deployment with multiple authenticated users it is a real privilege-boundary crossing. Worth an explicit decision: admission-time host policy, or follow the `DAYTONA_API_URL` precedent (operator env, not user-writable vault).

### 3. Doc advises an internet-exposed, limiter-less JSON instance (Low, advisory)

`docs/public/integrations/searxng.mdx` recommends `docker run -p 8080:8080` (binds `0.0.0.0`) with `json` whitelisted, and the troubleshooting section suggests "disable the limiter for trusted callers." That combination publishes an open search relay to the host's interfaces — the abuse pattern SearXNG's own limiter exists for. Recommend binding to loopback/the compose network instead.

## Checked and clean

- **No committed secrets**: docs use placeholders (`dtn_...`, `BSA...`); `.env.example` adds an empty `SEARXNG_URL=`; a regex sweep of the full diff found no credential-shaped literals.
- **No injection**: the model-controlled `query` reaches the URL only via `.query(&[("q", …)])` (percent-encoded by serde_urlencoded); no header, shell, or format-string sinks; `write!` calls use data args, not format strings.
- **No unsafe deserialization**: response parsed into `serde_json::Value`; formatters only extract string fields.
- **`secret_registry.rs` omission is correct**: the registry feeds only the env→vault migration, and unclassified URL-config matches the file's own precedent (`DAYTONA_API_URL`, `OPENAI_BASE_URL`).
- **No agent path to vault writes**: secrets route requires a user principal; run-scoped/worker principals don't satisfy it; standalone CLI reads env only.
- **Derived `Debug` on `SearchBackend`** (which would show `base_url`) is never rendered in production paths; `ToolSecrets` Debug redacts correctly (tested).
- **No new dependencies**; tests are httpmock-only (no live network); demo strings are static; no fallback to paid backends (consistent with the documented invariant).

Findings 1 and 2 are the ones worth acting on before merge; 1 is a straightforward fix, 2 is a design decision the repo should make consciously rather than inherit.