I now have everything needed. Here is the full sweep.

---

## Dimension-by-dimension sweep

### 1. Scope boundaries
- **Clearly in:** a third `web_search` backend with $0 marginal cost per query, wired through the existing single insertion point (`SearchBackend` enum in `lib/components/fabro-agent/src/web_search.rs`), its config plumbing, doctor diagnostics, and docs.
- **Clearly out:** changing the `web_search` tool name/schema (backends are invisible to the agent), changing Brave/Venice behavior, permission recategorization, touching `web_fetch`.
- **Genuinely unclear:** (a) does "runs locally" mean a self-hosted service (SearXNG-style) or just "free, no paid API" (keyless endpoint)? (b) does "set something up" mean Fabro should *operate* the service, or just *connect* to one? These produce materially different software — a JSON API client vs. an HTML scraper; a small backend PR vs. a container-lifecycle subsystem.

### 2. Users and callers
- Callers: agents (via the tool), operators (via vault/`fabro doctor`), the standalone CLI (`fabro exec` reads env).
- **Prior art found:** commit `53efde39` (Venice) then `88ed2ac9` (refactor) show the *exact* surface a new backend touches — and that the project deliberately deleted `[server.integrations.search] provider`/`venice_engine` settings and the `SearchProvider` enum in favor of presence-based selection from credentials. Reintroducing a provider enum would reverse a 3-week-old deliberate decision. Auto-selection must be preserved.
- Public surfaces at risk: OpenAPI spec **only if** config goes through settings.toml (the Venice commit touched `fabro-api.yaml`; the refactor removed it). Vault-only config needs no API changes — the secrets API already accepts arbitrary names (only bootstrap secrets are rejected).
- Demo mode (`fabro-server/src/demo/mod.rs:675`) hardcodes a string naming both env vars.

### 3. Data and state
- No persisted-data migration for the backend itself. No data written by previous versions is affected; rollback = remove the vault key.
- One wrinkle: `fabro-static/secret_registry.rs` `optional_vault_secrets()` drives a startup env→vault migration (`2026052501`). Whether a new key name joins that list is a convention decision (answerable from repo patterns, see bucket 2).

### 4. Existing behavior
- Verified current behavior: Brave > Venice selected once at session construction from `ToolSecrets`; **no fallback between backends on failure** (documented in `tools.mdx` and tested); tool omitted entirely with no credentials; Venice 400-char pre-HTTP rejection; server reads vault only (tested by `check_web_search_ignores_env_backed_*`), CLI reads env only.
- The goal is **extend**, not change: users with only Brave/Venice keys must see zero difference.

### 5. Edge and failure cases
- Goal says nothing about: local service down, SearXNG JSON format not enabled (403 — the classic gotcha; JSON must be whitelisted in `settings.yml`), slow instance, empty results, instance with bot-limiter requiring a key, `max_results` mapping (SearXNG has no count param — client-side truncation needed).
- Repo invariant answers the biggest one: selected backend errors out, no retry through others. Preserve.
- Guessing wrong is expensive only on: JSON-disabled handling (needs a precise, actionable error message) and timeout choice. Both implementation details.

### 6. Non-functional constraints
- Latency: SearXNG aggregation takes 1–5s vs Brave ~500ms; Venice already tolerates a 60s timeout, so a generous timeout is precedented. No stated perf expectations violated. Events strategy unaffected (existing `web_search` tool events already handled in CLI display code).

### 7. Security, privacy, permissions
- No permission widening (`web_search` stays Shell category / Full auto-approval).
- Data flow: queries move from Brave/Venice to the operator's instance — but SearXNG forwards to upstream engines, worth a docs note.
- The URL is operator-configured trusted config (same trust level as existing `base_url` settings) — not an SSRF widening. A URL is not a secret, but the vault-only policy for search config is deliberate and tested; where the URL lives is a repo-convention question (bucket 2).

### 8. Compatibility and versioning
- Backwards compatible as long as selection only adds a tier and existing no-local users are untouched. No feature flag needed. Nothing deprecated.
- Changelog entry is convention (dated `docs/public/changelog/2026-09-*.mdx`).

### 9. Testing and verification
- Existing tooling covers this well: httpmock unit tests per backend (request shape, response parse, error mapping), `from_secrets` precedence tests, diagnostics tests with in-memory vault. A live SearXNG E2E is the only new kind of test implied — optional, and my call.

### 10. Operational surface
- Doctor must learn the new backend (`check_web_search` in `fabro-server/src/diagnostics.rs`, which `fabro doctor` renders via the API). Docs: `tools.mdx`, new `integrations/*.mdx` + `docs.json` nav entry, `server-configuration.mdx`, changelog, demo string, possibly `.env.example`. No logging/metrics additions beyond existing style (web_search.rs currently has no tracing — match it).

### 11. Dependencies and integration
- No new Rust crates needed (`fabro_http` + `serde_json` suffice for SearXNG; a DDG scraper would want an HTML parser — `htmd` already in tree but it's html→markdown, not scraping). External: possibly a SearXNG container; possibly a compose profile. New config key name TBD.

### 12. Definition of done
- "Not what I asked for" scenarios: (a) built a scraper when they wanted a local service, or vice versa; (b) built config-only when they expected Fabro to make it *work* out of the box; (c) made local lowest-precedence so the Brave bill never shrinks; (d) shipped code but no way to actually run the service; (e) reintroduced the provider enum the team just deleted.

---

## Bucketing

**Answerable from the repository (answered):**
- Where the code lands and the full touch surface → the Venice commit's file list is the checklist.
- Selection philosophy → presence-based auto-selection; no provider enum (commit `88ed2ac9` deleted it deliberately).
- Where server-run search config must live → vault only (policy tested in diagnostics); standalone CLI reads env.
- Whether the secrets API accepts new names → yes, arbitrary non-bootstrap names.
- Whether a Fabro-managed long-lived container subsystem exists → no; sandbox manages per-run containers only. Fabro-managed service lifecycle would be greenfield.
- Self-hosted-service precedent → LiteLLM: operator runs it, Fabro points at `base_url`. Config-only.
- Whether OpenAPI must change → not if config stays vault-only.
- Doctor flow → server diagnostics endpoint, rendered by CLI.

**Yours to decide (decided — see below):** precedence (local-first), failure semantics (no fallback, preserve invariant), config key naming, timeout, truncation, error messages, optional instance API key, diagnostics probe shape, docs/changelog scope, test strategy.

**Genuinely theirs:** the local-search archetype (Q1) and service lifecycle ownership (Q2).

---

## Stress tests

- **Regret check:** the two rejection scenarios I couldn't resolve from the repo are exactly Q1 ("I meant free, not self-hosting") and Q2 ("I expected it to just work without me running a container"). Both added.
- **Second-round check:** things that would send me back — engine specifics (SearXNG is the only credible self-hosted JSON-API metasearch; Whoogle has no JSON API, YaCy has poor quality — not worth asking), auth on the instance (optional key, cheap to support), CLI scope (mirror existing env path — cheap). None survive as questions.
- **Merge check:** I initially had five candidates (engine, lifecycle, config surface, precedence, fallback). Config surface dissolved into repo convention; precedence and fallback dissolved into the cost motivation + documented invariant. Engine and lifecycle are orthogonal (engine choice matters even when operator-managed) and each materially changes the build — kept separate.
- **Self-answer check:** applied to each remaining question — Q1 I can't self-answer because "not as expensive" and "runs locally" genuinely point at different archetypes; Q2 I can't self-answer because "Can we set something up" is ambiguous between "configure" and "operate", and the repo precedent (LiteLLM = operator-managed) conflicts with the plain reading of the request.
- **Consequence check:** Q1 → JSON-API client + service docs vs. HTML scraper + no service vs. both; materially different code, docs, and failure modes. Q2 → ~small focused PR vs. a new container-lifecycle subsystem (pull/start/health/reuse/cleanup, non-Docker deployments get nothing); the largest scope difference in this whole analysis.

---

## Vetted questions

1. **What should the "local, cheap" search source actually be?**
   - **A — Self-hosted SearXNG** (recommended): free, real JSON API, aggregates Google/Bing/DDG so quality is high. Cost: operator runs a container; JSON output must be enabled in its `settings.yml`; personal instances can get rate-limited by upstream engines.
   - **B — Keyless free endpoint (e.g., DuckDuckGo HTML):** no service to run, $0. Cost: not actually "local" (still hits DDG), fragile scraping, rate limits/bot detection, ToS-gray.
   - **C — Both.** Cost: sum of both.
   - *What changes:* A builds a clean JSON backend + run-the-service docs (maybe a compose profile); B builds an HTML scraper with very different failure modes; C builds both.

2. **If a service is involved, who sets it up and keeps it running?**
   - **A — Operator-managed, LiteLLM-style** (repo precedent): Fabro gains the backend + a config key pointing at an existing instance; docs explain running SearXNG. Cost: smallest PR, but the operator does the setup.
   - **B — Fabro-managed:** Fabro auto-starts/reuses a SearXNG container via the already-mounted `docker.sock` when no other backend is configured. Cost: a new container-lifecycle subsystem (image pull, start, health, reuse, cleanup, upgrades); non-Docker deployments get nothing; much bigger PR and review surface.
   - *What changes:* A is a backend + config + docs change; B adds greenfield infrastructure to the sandbox/Docker layer — the difference between a focused PR and a small project. (Only meaningful if Q1 ≠ B-only.)

## Decided without asking

- **Precedence: local-first when configured.** The stated goal is cost; if a paid key could still win, the feature fails its purpose. Users who don't configure local see zero change; new precedence tests will pin `local > brave > venice`.
- **No fallback between backends on failure.** Documented, tested invariant; a dead local instance errors the tool call, same as a dead Brave key does today.
- **No provider enum / no `[server.integrations.search]` resurrection.** Commit `88ed2ac9` deliberately removed it three weeks ago; presence-based selection stays.
- **Config surface: vault key for server runs (e.g. `SEARXNG_URL`), process env for the standalone CLI** — mirrors the existing two keys exactly and respects the tested vault-only policy; avoids all OpenAPI/TS-client regeneration.
- **Tool name, schema, permission category, events: unchanged.** The backend remains invisible to the agent.
- **Diagnostics:** extend `check_web_search` to probe the local endpoint (reachable → pass; else warning with actionable remediation mentioning the URL key); update the demo-mode string.
- **Result mapping:** SearXNG `results[].{title,url,content,publishedDate}` → existing `SearchHit`; client-side truncation to `max_results`; optional instance API key supported as a second optional vault entry.
- **Testing:** httpmock unit tests mirroring the Brave/Venice suites + precedence tests + diagnostics tests; no new live E2E tier (SearXNG schema pinned in fixtures).
- **Docs:** new `integrations/searxng.mdx` (+ `docs.json` nav), `tools.mdx` and `server-configuration.mdx` updates, changelog entry, `.env.example` line for the CLI path.