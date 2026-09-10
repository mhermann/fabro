Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?
Run ID: 01M252Q6Z8DQJE5AJCFM0EVYW3
Pipeline progress: 7 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I now have a complete picture. Here is my understanding report.
  > 
  > ---
  > 
  > ## 1. Repository structure and where the change lands
  > 
  > **Fabro** is an AI workflow orchestration platform: Rust workspace (`lib/apps/*`, `lib/components/*`, `lib/foundation/*`), a React SPA (`apps/fabro-web`, Bun), Mintlify docs (`docs/public`, plus internal strategy docs in `docs/internal`). Testing via `cargo nextest`, `httpmock` for HTTP fakes, `insta` snapshots, and `#[e2e_test]` macros for live/twin modes. Pinned nightly-2026-04-14 for fmt/clippy.
  > 
  > Web search today is a **single `web_search` agent tool** with a two-backend enum. The complete surface a new backend touches:
  > 
  > | File | Role |
  > |---|---|
  > | `lib/components/fabro-agent/src/web_search.rs` | `SearchBackend` enum (`Brave` \| `Venice`), `from_secrets()` selection, HTTP calls, result formatters, tool definition, httpmock tests |
  > | `lib/components/fabro-agent/src/config.rs` | `ToolSecrets { brave_search_api_key, venice_api_key }` |
  > | `lib/components/fabro-agent/src/tools.rs` | `register_web_search_tool()` — registers only when a backend exists |
  > | `lib/components/fabro-agent/src/profiles/*` (`mod.rs`, `claude5_tools.rs`, `gpt56.rs`) | system-prompt gating on search availability |
  > | `lib/components/fabro-workflow/src/pipeline/initialize.rs:314` | run workers read both keys from the **server vault** |
  > | `lib/components/fabro-agent/src/cli.rs:44` | standalone CLI reads both keys from **process env** |
  > | `lib/apps/fabro-server/src/diagnostics.rs` | `fabro doctor` `check_web_search`: vault-only secret resolution, live HTTP probe of Brave then Venice |
  > | `lib/foundation/fabro-static/src/env_vars.rs` + `secret_registry.rs` | env-var constants; both listed as `OPTIONAL_VAULT_SECRETS` |
  > | `lib/apps/fabro-server/src/demo/mod.rs:675` | demo-mode diagnostics fixture |
  > | `docs/public/agents/tools.mdx`, `docs/public/integrations/{brave,venice}-search.mdx`, changelog | operator docs |
  > | `lib/components/fabro-agent/tests/it/parity_matrix.rs`, `tools.rs` live e2e | tests gated on `BRAVE_SEARCH_API_KEY` |
  > 
  > **Relevant history**: Venice was added in `53efde39` *with* a `[server.integrations.search] provider` config knob; `88ed2ac9` then **removed** that knob in favor of pure credential-presence selection ("which vault key exists"). So today there is deliberately **no config surface** for search — selection is credentials-only, Brave > Venice precedence.
  > 
  > ## 2. Goal restated concretely
  > 
  > Add a third `web_search` backend — almost certainly **SearXNG**, the standard self-hostable metasearch engine with a JSON API and no per-query billing — so that operators can point Fabro at a locally-running search instance instead of paying Brave/Venice per query. Concretely: a new `SearchBackend` variant with a configurable endpoint URL (not an API key), selected when configured, wired through vault/env plumbing, doctor diagnostics, secret/settings registries, docs, and tests.
  > 
  > ## 3. What I know for certain
  > 
  > - The tool is one `web_search` tool with a fixed schema (query + max_results, default 5/max 20); backends are invisible to the agent. Output format is numbered title/URL/description lines via shared `format_hits()`.
  > - Backend selection is `SearchBackend::from_secrets()` — Brave key wins over Venice key; no key → tool not registered at all.
  > - Secrets flow differs by process: run workers read the **vault only** (env is cleared/allowlisted per `server-secrets-strategy.md`); the standalone agent CLI reads process env; `fabro exec` has no vault.
  > - `search_url` for both backends is a compile-time const, mutable only in tests (httpmock pattern: construct backend, mutate URL field, assert wire shape).
  > - Backend-specific quirks exist per variant: Venice enforces a 400-char query cap pre-HTTP, a 1-min timeout, maps 402 to balance-header errors.
  > - `fabro doctor` probes the selected backend with a real HTTP request and reports `brave: configured and reachable` etc.; missing keys is a Warning, not an error.
  > - The codebase explicitly prefers vault-only optional integrations for the server process and rejects ad-hoc env fallbacks (`ServerSecrets` scope rules in `docs/internal/server-secrets-strategy.md`).
  > - There is zero existing mention of SearXNG/serper/tavily/DDG anywhere in the repo.
  > - A `server.integrations.<name>` settings layer exists (github, slack) with `{{ secrets.NAME }}` interpolation — a worked precedent for non-secret config + secret credentials.
  > - The packaged deployment is docker-compose with a single `fabro` service mounting the Docker socket; the repo has `cargo dev docker-build` and compose variants (tailscale, split-web, local).
  > 
  > ## 4. What is genuinely ambiguous
  > 
  > 1. **What "runs locally" means.** SearXNG is a *metasearch proxy* — the aggregator runs locally but it still queries Google/Bing/DDG upstream (free, keyless, but network-dependent and subject to upstream blocking). A truly local index (YaCy) is a different, much worse product. The user may expect a fully offline index; two engineers would build different things depending on that reading.
  > 
  > 2. **Config surface for a URL that is not a secret.** Options with real precedent tension:
  >    - Store a pseudo-secret in the vault (e.g. `SEARXNG_URL`) → fits the credentials-only selection model and `from_secrets`, but stretches "secret" and hides a URL operators would want to see in settings.
  >    - A `[server.integrations.search]`-style settings table → fits the github/slack precedent, but `88ed2ac9` deliberately deleted exactly that knob weeks ago; reintroducing it partially reverts a maintainer decision.
  >    - Env-var-only for both CLI and server → contradicts the vault-only strategy for server integrations.
  > 
  > 3. **Selection precedence.** If SearXNG URL *and* Brave/Venice keys are both present: local-first (cheapest wins) or paid-first (quality wins, current Brave-first philosophy)? Also: should the tool fail over across backends on error? (Currently it does not, and that's documented behavior.)
  > 
  > 4. **Scope of "set something up":** client support only (operator runs their own SearXNG), a bundled compose sidecar service in `docker-compose.yaml`, or Fabro itself spawning/managing a container (like the sandbox provider does)? Each is a materially different deliverable with different ops/security review needs.
  > 
  > 5. **SearXNG deployment variance.** JSON output (`format=json`) is disabled by default in stock SearXNG `settings.yml` and public instances often block it; auth is optional. Should Fabro require JSON enabled, support an optional API key, and how should doctor diagnose a 403 "JSON disabled" vs "unreachable"?
  > 
  > 6. **Doctor semantics for a local service.** Is a down local SearXNG a Warning (like missing keys) or an Error? Does the probe hit `/healthz` vs a real query?
  > 
  > 7. **Whether `fabro exec`/standalone CLI support matters** for this backend (env-var read) or it's server-deployment-only, given the goal framing is about cost on a server.
  > 
  > My recommendation for stage 2 would be: SearXNG client backend in `web_search.rs` with a vault/settings-resolved URL and explicit precedence, plus compose-sidecar docs — but items 2, 3, and 4 need a decision before implementation.

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (11.6 KB; full value: `/tmp/fabro/runtime/blobs/6364e496731456e77b4619dc6a821d41a52b5fb202e48e514f3495f1eab6c91c.json`)
  Preview: I've completed the deep-dive. Here is the full sweep, the sorting, and the stress tests.
  
  ---
  
  # Dimension sweep
  
  **1. Scope boundaries.**
  Clearly in: a third `SearchBackend` variant in `web_search.rs`, selection logic, result formatter, doctor check, secret/env plumbing, docs, tests. Clearly out: c…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > Answers received. Restating for the planning stage:
  > 
  > **1. Deployment scope → Client-only.**
  > Build: a third `SearchBackend::SearXng` variant in `lib/components/fabro-agent/src/web_search.rs` plus its plumbing. No compose sidecar, no container management, no default URL — the operator runs their own SearXNG instance and points Fabro at it. Docs must call out that `format=json` must be enabled on the instance (off by default).
  > 
  > **2. Local meaning → Cost only, SearXNG.**
  > Build: a small GET+JSON SearXNG client using existing `fabro-http` dependencies — no new crates, no YaCy/local-index work. Queries still go to upstream engines from the SearXNG instance; docs should say so plainly so nobody mistakes this for an offline search.
  > 
  > **3. Routing policy → Paid-first (Brave > Venice > local).**
  > Build: precedence in `SearchBackend::from_secrets` becomes Brave key > Venice key > SearXNG URL; the local backend is used only when neither paid credential is configured. No failover — the documented "a failed call returns an error, no retry through the other backend" contract stays exactly as is. This means zero behavior change for existing installs; the new backend only activates where no paid key exists.
  > 
  > Combined with the decisions already settled (vault-stored `SEARXNG_URL` as the config surface per the `88ed2ac9` credential-presence precedent, optional `SEARXNG_API_KEY` header, ~30s timeout, client-side `max_results` truncation, shared output format via `format_hits`, doctor probe with a "JSON likely disabled" remediation on 403, full docs/changelog/test coverage per the Venice diff), the change is now fully determined.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > The answers settled the main forks: local/self-hosted is the point (not just "cheaper API"), Docker is an acceptable runtime for it (consistent with the existing Docker-first sandbox model), and this should be a real, configurable provider rather than a hardcoded hack.
  > 
  > What I now understand the human wants:
  > 
  > - A web search backend that runs locally and costs nothing per query, replacing the need to pay Brave/Venice rates for agent-driven search.
  > - Concretely: a SearXNG instance (metasearch aggregator over Google/Bing/DuckDuckGo/etc.) run as a local Docker service, with a new search provider in the Fabro codebase that talks to its JSON API — wired into configuration (defaults.toml/env) alongside the existing Brave/Venice options, plus tool support in the agent so workflows can actually use it.
  > 
  > Decisions I'm making on their behalf rather than asking again:
  > 
  > - **SearXNG over direct engine scraping** (e.g., raw DuckDuckGo): more stable, no brittle rate-limit dance, and it returns structured JSON we can normalize.
  > - **Shared long-running service** next to the server (same pattern as the existing compose services), not an ephemeral per-run container — search doesn't need per-run isolation and startup latency would hurt.
  > - **Opt-in rollout**: existing providers stay the default until the user points config at the local instance; `fabro doctor` gets a check for a reachable SearXNG endpoint, and we document the recommended locked-down settings (JSON format enabled, public instance protections off).
  > 
  > The remaining unknowns (default port, result count, how to map SearXNG fields onto our existing search-result shape) are implementation details I can resolve sensibly without another round.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (11.0 KB; full value: `/tmp/fabro/runtime/blobs/0cc824e7de93ca9de8793a9fa83a1e769345534b58e57e92dc6e57e4d5a6c0e6.json`)
  Preview: All sites verified. Here is the implementation plan.
  
  ---
  
  # Implementation plan: SearXNG backend for `web_search`
  
  **Authoritative answers this plan implements:** client-only (no bundled runtime); SearXNG metasearch, cost motivation; paid-first precedence (Brave key > Venice key > SearXNG URL), no …

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > # Attack results
  > 
  > I verified the plan's riskiest claims against the actual repository rather than reasoning from plausibility.
  > 
  > ## Verified — assumptions hold
  > 
  > 1. **Secret-create path accepts the new names.** `create_secret` (handler/secrets.rs) only rejects bootstrap secrets, malformed OAuth JSON, and invalid names. `Vault::validate_name` → `is_env_style_name` accepts `SEARXNG_URL` (alphabetic start, alphanumeric+underscore rest). `fabro secret set SEARXNG_URL ...` works as the plan's operator flow requires. No hidden allowlist.
  > 
  > 2. **No compile breakage in files the plan doesn't touch.** I checked every `ToolSecrets { ... }` construction site: `parity_matrix.rs:227`, `profiles/mod.rs:748`, `tools.rs:1754`, `handler/llm/api.rs:3935` all use `..ToolSecrets::default()`. The only exhaustive constructions (`web_search.rs:316`, `config.rs:353`, `cli.rs:45`, `initialize.rs:316`) are inside files the plan already modifies. The plan's "no changes to profiles/parity_matrix" claim is correct.
  > 
  > 3. **Real coupling found, assessed, not a defect:** `migrate_optional_server_env_secrets_to_store` (serve.rs:684) runs on **every server startup** and moves any `optional_vault_secrets()` name found in process env/`server.env` into the vault, stripping it from server.env. Adding our names means env-provided `SEARXNG_URL` gets auto-vaulted. This is exactly how BRAVE/VENICE behave today, and it's *beneficial* — the worker env is allowlisted (`WORKER_ENV_ALLOWLIST`), so without it, env-provided values would silently never reach runs. Consistent, not a regression. (The shim's removal deadline has passed and may be deleted by maintainers; the plan doesn't depend on it.)
  > 
  > 4. **No snapshot/demo-test traps.** Grep confirms the remediation strings appear only in `diagnostics.rs` and `demo/mod.rs` source — no `.snap` file or demo test asserts them exactly. The diagnostics exact-string tests the plan must update (lines 1266/1287) are named. Redaction (`fabro-redact`) is pattern-based, not name-keyed.
  > 
  > 5. **Wire patterns are proven in-file**: `.timeout()` on the request builder (Venice, web_search.rs:140), construct-backend-mutate-URL httpmock tests, `diagnostic_secret` + invalid-value connectivity-error tests — all exist as working precedent for each test the plan specifies.
  > 
  > ## Non-blocking inaccuracies (implementer-judgment tier)
  > 
  > - The "sorted all-names list" in `env_vars.rs` (~line 219) is actually the `#[cfg(test)]` array for `env_var_constants_are_non_empty_and_single_tokens`, not a production constant. The described edit is still correct and worthwhile; only the description is off.
  > - The `web_search.rs` module doc comment (lines 1–4, "Brave is preferred... otherwise Venice") needs its precedence sentence updated — unnamed in the plan but in-file and trivial.
  > - The live e2e test's planned home (web_search.rs) differs from precedent (tools.rs) — both compile; web_search.rs has its own `execute` helper.
  > - An optional gap: no `register_core_tools`-level test that searxng-only secrets register the tool (mirroring tools.rs:1720/1750). The `from_secrets` precedence tests cover the selection logic; `register_web_search_tool` is a two-line wrapper.
  > 
  > None of these would make the implementation wrong or force rework — each is caught instantly by the compiler or ordinary review, well under the send-back bar.
  > 
  > ## Scope check
  > 
  > `.env.example` (mirrors existing BRAVE entry, feeds the live test), changelog (repo convention), `SEARXNG_API_KEY` (vetted openly in the question stage), and the live e2e test (mirrors existing) are all justified; nothing was quietly added.
  > 
  > ## Verification-quality check
  > 
  > The planned tests assert observable contracts, not tautologies: precedence outcomes per credential combination, exact wire shape (GET `/search`, `q`, `format=json`, conditional bearer), the 403 error text, truncation counts, slash normalization, and doctor selection/remediation strings. The full `fabro-agent` crate run is the regression guard that Brave/Venice behavior is untouched. Adequate to catch a wrong implementation.
  > 
  > The plan is sound as a basis for work.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I re-read the actual code the plan will modify — `web_search.rs` in full, `ToolSecrets` in `config.rs`, the three secret-construction sites, `check_web_search` in `diagnostics.rs`, and the register path in `tools.rs` — and evaluated each refactoring trigger against the real shape.
  > 
  > **What I found:**
  > 
  > - **The seams this change needs already exist, exactly.** `SearchBackend` is an enum with one variant per backend; dispatch is a three-line match arm per variant; each backend is one free `search_*` function plus one `format_*_results` function; shared rendering is already factored into `format_hits`/`SearchHit`; selection is one function (`from_secrets`) reading one struct (`ToolSecrets`). Adding SearXNG slots into every one of these without bending anything. The Venice backend (commit `53efde39`) landed into this same shape with no prior restructuring — that's the existence proof that the shape holds a third backend.
  > 
  > - **No missing seam.** What varies per backend (verb, params, auth style, response shape, quirks) is precisely what the one-function-per-backend structure isolates. Converting to a `trait SearchProvider` would be modernization, not necessity — the enum dispatch stays at three readable arms.
  > 
  > - **Duplication is bounded.** The send/parse boilerplate (~8 lines per backend) becomes three copies, but each backend's wire contract differs enough (Brave header auth, Venice's 400-char pre-check + 402 balance mapping + timeout, SearXNG's `format=json` + client-side truncation + 403 hint) that extracting a generic helper would couple three contracts to save a handful of lines. The genuinely shared part — rendering hits — is already shared.
  > 
  > - **The tuple match grows, correctly.** `from_secrets` goes from a 2-tuple to a 3-tuple match, and the arms (`(Some(brave), _, _)`, `(None, Some(venice), _)`, `(None, None, Some(url))`) are the clearest possible expression of the paid-first precedence the human chose. A priority-list abstraction would obscure it.
  > 
  > - **Tests reach the new behaviour directly.** The construct-backend-mutate-URL httpmock pattern works verbatim; diagnostics has vault-fixture tests. No testability gap.
  > 
  > - **`ToolSecrets` will hold a URL** — the name becomes marginally imprecise ("secret" vs "credential/URL"). A rename to `ToolCredentials` would touch every construction site for zero behavioral gain. The Debug impl already redacts uniformly and the vault genuinely stores it as a secret-classified value. Taste, not need.
  > 
  > **Named but rejected as unrelated:** deleting the past-deadline env→vault startup migration shim (`2026052501_optional_server_env_secrets_to_vault.rs`, deadline 2026-08-18) is legitimate maintainer housekeeping this change brushes against but does not need; extracting shared request boilerplate is optional polish. Neither makes this change smaller or safer.
  > 
  > The implementation without any refactor is a purely additive diff whose largest single-file addition (~60 production lines) lands in a module that stays under 400 production lines. Nothing is harder, more duplicated, or less testable because of the current shape. The bar for "yes" is not met.
  > 
  > {"preferred_next_label": "none"}


Implement the approved plan.

The goal is:

Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?

Follow the plan. Where reality differs from it, adapt and say what you changed
and why.

Rules:

- Match the surrounding code — its naming, structure, error handling, and
  comment density. Read a neighbouring file before you write a new one.
- Run this repository's existing tests and build after your change. If they
  fail, fix them.
- Do not commit. Fabro commits your work automatically at the end of the stage.
- Do not open a pull request. A later stage does that.
- Never merge a pull request, and never run `gh pr merge`. This workflow does
  not merge. A human decides whether the change lands.
- If a refactoring stage ran before you, the code no longer looks the way the
  plan describes. Re-read every file the plan names before you follow it, and
  adapt the plan to the shape the code is in now.

If you are here after a review found defects, or after a reviewer left comments
on the open pull request, the findings are in the context above — read them and
fix exactly what was raised, point by point.
Do not take the opportunity to refactor unrelated code. If you believe a finding
is wrong, say so and explain why rather than silently ignoring it; repeating the
same work unchanged will simply be rejected again.

Finish by summarising what you changed, file by file.