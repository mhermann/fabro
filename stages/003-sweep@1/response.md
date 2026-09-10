I've completed the deep-dive. Here is the full sweep, the sorting, and the stress tests.

---

# Dimension sweep

**1. Scope boundaries.**
Clearly in: a third `SearchBackend` variant in `web_search.rs`, selection logic, result formatter, doctor check, secret/env plumbing, docs, tests. Clearly out: changing the `web_search` tool schema (it's deliberately shared across backends), touching Brave/Venice wire code, permission model changes. Genuinely unclear: whether *"set something up that runs locally"* means Fabro should **ship/operate** the search runtime (compose sidecar or managed container) or only **point at** one the operator runs. These two readings produce materially different software (deployment surface + settings defaults vs. pure client code). There is sidecar precedent: `docker-compose.prod.yaml`/`split-web.yaml` bundle Caddy services, so a SearXNG sidecar is idiomatic if wanted.

**2. Users and callers.**
Agents call `web_search` (schema unchanged — the enum is invisible to them). Operators configure via `fabro secret set` / env. `fabro doctor` renders diagnostics. The web UI only maps the tool name to a label (`run-stages.tsx:999`) — no change. The OpenAPI spec is untouched *unless* we reintroduce a settings table (the deleted `SearchIntegrationSettings` lived in the spec — see dimension 8). Existing Brave/Venice behavior must not change when the new backend is unconfigured.

**3. Data and state.**
No persisted data, no migrations. Vault entries are operator-created on demand; rollback = removing the entry (tool falls back to current behavior). Nothing we write survives uninstall beyond that entry. Low risk.

**4. Existing behavior.**
Verified, not assumed: selection is credentials-only (`from_secrets`), Brave > Venice, tool omitted entirely with no credential, and — documented in `tools.mdx:137` — **no failover**: a failed call errors rather than retrying through the other backend. A maintainer commit (`88ed2ac9`, *"select backend from available credentials"*) deliberately **removed** a `[server.integrations.search]` config knob three weeks ago, along with its API schemas. The goal is an extension, but it forces two unstated choices: where the local backend sits in precedence, and whether the no-failover contract holds when a free local instance is flaky.

**5. Edge and failure cases.**
Local service down (mid-run tool error), SearXNG `format=json` disabled (common — stock config and most public instances return 403), empty results, slow instance, URL normalization, `max_results` (SearXNG's API has no count param → client-side truncation), optional auth. The goal says nothing about failover or about down-local UX; guessing wrong on routing policy here is the expensive guess. The rest are cheap to handle and follow existing per-backend quirk precedent (Venice's 400-char cap, 402 balance mapping).

**6. Non-functional constraints.**
No stated tool performance budgets. Local latency is a benefit, not a risk. Needs a timeout (Venice uses 1 min; local should be tighter). No memory/concurrency concerns — one GET per call.

**7. Security, privacy, permissions.**
No permission widening (same tool, same `shell` category). Key nuance: SearXNG is a **metasearch proxy** — the instance runs locally but queries still go to Google/Bing/DDG upstream. So "runs locally" ≠ "queries stay on your network"; the privacy reading and the cost reading diverge here. A URL stored in the vault gets redaction for free. A bundled sidecar would sit on the compose-internal network with no auth needed.

**8. Compatibility and versioning.**
Additive and default-off. The one compatibility landmine is config surface: reintroducing `[server.integrations.search]` would reverse the maintainer's own recent refactor — avoidable by using the vault (credential-presence model stays intact). No deprecations, no feature flags needed.

**9. Testing and verification.**
Existing tooling covers this fully: httpmock wire tests in `web_search.rs` (construct backend, mutate URL, assert request/response), `from_secrets` precedence tests, diagnostics tests with `vault_entries` fixtures, formatter unit tests, and the live-e2e pattern (`#[e2e_test(live("ENV"))]`). No new test kind implied. SearXNG's API is a plain GET+JSON — no new dependencies; `fabro-http` suffices.

**10. Operational surface.**
Repository convention requires: dated changelog `.mdx`, `docs.json` nav entry, new `integrations/searxng.mdx` (or equivalent), `tools.mdx` update, diagnostics summary string, demo-mode fixture (`demo/mod.rs:675`), troubleshooting section. Logging: a debug-level line for backend selection fits the logging strategy; no new events (tool events already exist). The full file list is proven by the Venice diff (`53efde39`).

**11. Dependencies and integration.**
No new Rust crates. External: a SearXNG runtime (image `searxng/searxng` if bundled). One new vault/env name. No infra otherwise.

**12. Definition of done.**
Rejection scenarios despite green tests: (a) "I wanted it to work out of the box" — no bundled runtime; (b) "I wanted it pointed at my existing instance" — we shipped a sidecar they didn't want; (c) "I thought queries would stay on my network" — metasearch still calls upstream; (d) "I wanted to stop paying for Brave" — local configured but lowest precedence changes nothing. Each maps to a question below.

# Sorting

**Answerable from the repository** (answers found, not asked):
- Selection model, precedence, and the no-failover contract → `web_search.rs`, `tools.mdx`.
- Config-surface direction → `88ed2ac9` removed the settings knob on purpose; credential-presence selection is the maintainer's chosen model. Vault-stored URL it is.
- Worker vs CLI secret channels → worker reads vault (`pipeline/initialize.rs:314`), standalone CLI reads env (`cli.rs:44`); both paths needed, both patterns exist.
- Sidecar precedent → Caddy services in prod/split-web compose.
- Exact file/documentation/test surface → enumerated from the Venice commit diff.
- No migrations, no API changes (given the vault approach).

**Yours to decide** (decided; see final section): vault key naming, optional API-key support, timeout, client-side result truncation, URL normalization, doctor probe shape and status mapping, formatter field mapping, CLI env read, demo string, changelog/docs dates, debug log line, per-result engine filtering.

**Genuinely theirs** → the three questions below.

# Stress tests

- **Regret check**: the four rejection scenarios above produced exactly Q1 (scope), Q2 (network expectation), Q3 (routing). A fifth candidate — config surface — was defused by the repo's own answer (`88ed2ac9`).
- **Second-round check**: "sidecar default-on or opt-in?" folded into Q1's options; "which SearXNG engines?" — decided (SearXNG defaults, not configurable v1); "JSON disabled?" — doctor + docs, decided. Nothing else would force a second round.
- **Merge check**: engine-brand + privacy merged into one network-expectation question; precedence + failover merged into one routing-policy question.
- **Self-answer check**: config surface removed (repo answers it); CLI/exec support removed (existing pattern answers it); API-surface question removed (falls out of the vault decision).
- **Consequence check**: each remaining question states what gets built differently per answer, below.

---

## Vetted questions

**1. Deployment scope — what does "set something up that runs locally" cover?**
- **A. Client-only**: add the backend; the operator runs SearXNG themselves and stores the URL (`fabro secret set SEARXNG_URL http://...`). Cheapest; requires the operator to know SearXNG's `format=json` must be enabled.
- **B. Bundled sidecar**: A + a `searxng` service in `docker-compose.yaml` (JSON pre-enabled, compose-internal network, healthcheck) and a documented default URL. Follows the existing Caddy sidecar precedent; one more container operators inherit.
- **C. Fabro-managed runtime**: the server spawns/supervises a SearXNG container like the sandbox provider does. Heaviest — new lifecycle, image pulls, cleanup; likely overkill.
Build difference: A is pure Rust client code + docs; B adds deployment assets and a default URL; C adds a container-management subsystem.

**2. What does "runs locally" need to mean — cost only, or network privacy?**
- **A. Cost/free results (recommended)**: SearXNG metasearch — the instance is local and keyless, but queries still go to Google/Bing/DDG upstream. Good result quality, no billing, requires internet.
- **B. Queries must not leave the network**: a truly local index (e.g. YaCy) — offline-capable but substantially worse result quality and a heavier runtime; a different protocol and different engine to integrate.
Build difference: A is a ~100-line GET+JSON client; B is a different engine, different client, different ops story — and I'd push back that agent search quality would suffer.

**3. When a local backend and a paid key are both configured, who serves queries — and what happens when the local one fails?**
- **A. Local-first, no failover**: cheapest; a down local instance returns tool errors even though a Brave key exists (consistent with today's documented no-retry contract).
- **B. Local-first with failover to Brave/Venice on error**: best cost/reliability balance; changes the documented "no retry through the other backend" behavior for all backends or just this one — a product-visible contract change.
- **C. Paid-first (Brave > Venice > local)**: zero risk to existing users, but configuring the local backend then changes nothing for any install that already has a key — likely defeats the stated goal.
Build difference: A/C are a precedence-arm insert in `from_secrets`; B additionally restructures `search()` into a backend chain and updates the documented contract.

## Decided without asking

- **Config surface = vault, not a settings table.** `88ed2ac9` deliberately deleted `[server.integrations.search]`; a vault-stored URL preserves the credential-presence selection model and requires no API/settings changes.
- **Vault/env key name: `SEARXNG_URL`** (works identically in `fabro secret set` and standalone-CLI env), listed in `EnvVars` + `secret_registry` as an optional vault value.
- **Standalone CLI/`fabro exec` support: yes**, via process env — matches `cli_tool_secrets` for the existing keys.
- **Optional `SEARXNG_API_KEY`** sent as a header when present — symmetric with the other backends' credential shape, and needed for any non-localhost instance.
- **`max_results`**: client-side truncation (SearXNG has no count param). **Timeout**: ~30s (local should beat Venice's 1-min remote budget). **URL normalization**: strip trailing slash.
- **Formatter**: `title`/`url`/`content`→description, optional `publishedDate`→date, via the shared `format_hits` — identical agent-visible output.
- **Doctor**: probe the configured instance with a real 1-result query; report `searxng: configured and reachable` / HTTP status (a 403 gets a pointed "JSON format likely disabled" remediation); down instance = Warning, matching missing-key severity. Demo-mode fixture string updated.
- **No per-engine configuration v1** — SearXNG's default engine set; query-length and limits stay governed by the existing tool schema.
- **Docs/changelog**: dated changelog entry, `docs.json` nav + `integrations/searxng.mdx`, `tools.mdx` precedence paragraph update, troubleshooting section.
- **Logging**: one debug-level line recording backend selection; no new events, no secrets logged.
- **Tests**: httpmock wire tests, `from_secrets` precedence tests, formatter tests, diagnostics vault-fixture tests; optional live e2e gated on `SEARXNG_URL`. No new dependencies, no migrations.