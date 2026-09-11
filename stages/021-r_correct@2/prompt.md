Goal: Add support for forgejo similar to github integration
Run ID: 01M27HQ4ZCMNF8T64PSGESXABZ
Pipeline progress: 12 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (8.6 KB; full value: `/tmp/fabro/runtime/blobs/438d2ca833e28212e413fc9ce7598ae9a92103303fd1a15be6f33ca84a89e372.json`)
  Preview: Exploration complete. Here is my understanding.
  
  ## 1. Repository structure and where the change lands
  
  **Language/tooling:** Rust workspace (crates in `lib/apps`, `lib/components`, `lib/foundation`) + TypeScript/React 19 web app (`apps/fabro-web`, Bun). OpenAPI-first API (`docs/public/api-reference…

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (14.6 KB; full value: `/tmp/fabro/runtime/blobs/0dd3cad2b1b6e6023c89191fe2d0320423f45d6c420b6b311e81a1f04f8b31d9.json`)
  Preview: I've now read the remaining integration surfaces (automation triggers, install flow, OAuth login, config defaults, API wire format, git bridge, PR pipeline). Here is the full sweep.
  
  ## The sweep, dimension by dimension
  
  ### 1. Scope boundaries
  "Similar to github integration" maps to a ten-surface s…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > All five answers are back. Restatement for the planning stage:
  > 
  > ## Answers and what they mean
  > 
  > 1. **Scope — B: Core + tokens, webhooks, install, diagnostics.** Build: `fabro-forgejo` credentials, clone into Docker/Daytona sandboxes, PR create/get/merge/close (API + web UI), `[server.integrations.forgejo]` settings, sandbox token minting (`run.integrations.forgejo` → `FORGEJO_TOKEN`), webhook route, install flow (CLI + web wizard), doctor/diagnostics, docs. **No OAuth2 login, no automation-target changes** (those were scope C), and the tracker stays out regardless.
  > 
  > 2. **Instances — A: Single instance per server.** One URL + one PAT in server settings; `owner/repo` slugs are reused scoped to that instance. No wire-format change to `GitRunTarget`/`RunTarget`, no persisted-data migration, no host-qualified identity. Non-configured hosts are rejected.
  > 
  > 3. **Auth — A: PAT only.** One credential variant (`Authorization: token <PAT>`). No OAuth2 app machinery, no Forgejo Apps, no version gate beyond current stable Forgejo.
  > 
  > 4. **Architecture — A: Additive parallel crate.** New `lib/components/fabro-forgejo` mirroring `fabro-github`, with explicit dispatch at boundaries. GitHub code paths, wire formats, and persisted data remain untouched — zero GitHub regression risk; duplicated shape is accepted.
  > 
  > 5. **Webhooks — A: Inert parity.** The Forgejo webhook route verifies HMAC and logs the delivery, exactly like the GitHub route. No event-driven triggering, no new security surface.
  > 
  > Standing decisions from the sweep that remain in force: HTTPS-only instance URLs; accept `X-Forgejo-*`/`X-Gitea-*`/GitHub-compat webhook headers (all HMAC-SHA256); `MergeStrategy` maps directly (merge/squash/rebase), auto-merge endpoint verified against the real API during implementation; env vars `FORGEJO_URL`/`FORGEJO_TOKEN` registered in `fabro-static` env_vars + secret registry; integration default-off via `enabled` flag; tests via ported `MockHttpClient` + env-gated live tests; OpenAPI-first workflow for any spec changes; docs + changelog per repo convention.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > The answers covered the material questions: scope mirrors the existing `fabro-github` integration (auth + PR creation), Forgejo uses token-based auth rather than GitHub App JWT/installation tokens, and self-hosted base URLs must be first-class.
  > 
  > What I understand the human wants, including decisions I'm making on their behalf:
  > 
  > - **New `fabro-forgejo` crate** parallel to `fabro-github**: a Forgejo API client covering token auth (PAT, with base-URL config for self-hosted instances), PR creation/update, and issue comments — the same product surface `fabro-github` provides today, adapted to Forgejo's simpler token model.
  > - **Configuration and secrets**: a `forgejo` config section (base_url + token) loaded through the existing server-secrets startup/validation path, not a new secrets mechanism.
  > - **Shared abstraction kept minimal**: I'll introduce a narrow trait (e.g., PR creation) only where both forges must be interchangeable for callers, rather than a speculative full forge abstraction. The crates stay separate.
  > - **Sandbox clone-source contract stays GitHub-only** in this change; extending clone-based providers (Docker/Daytona) to Forgejo origins is a follow-up with its own admission/clone semantics. This is the one scope line I'm drawing myself, since it's separable and touches a documented contract.
  > - **Testing**: twin/scripted HTTP tests like other integration crates; no live Forgejo dependency in CI.
  > 
  > Remaining gaps (exact Forgejo API versions, OAuth2 app flow vs PAT-only) don't change the shape of what gets built — PAT-first with the client structured so an OAuth2 flow can be added later.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (20.8 KB; full value: `/tmp/fabro/runtime/blobs/2629b1ff29c3eaff078eeb63c23dd8fc1018e1afa026629cd693ec18cba8e129.json`)
  Preview: I have verified every integration point against the current code. Here is the implementation plan.
  
  ---
  
  # Implementation Plan: Forgejo integration (scope B, single instance, PAT-only, additive crate, inert webhooks)
  
  ## Governing decisions (from the human's answers)
  
  - **Scope B**: credentials, clo…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the plan against the repository as it actually is. Findings, then the verdict.
  > 
  > ## Defects found
  > 
  > **1. `fabro-manifest` is omitted entirely.** `lib/components/fabro-manifest/src/lib.rs` consumes `normalize_repo_origin_url` and `parse_github_owner_repo` at sites ~382, 418 (`parse...ok()?` — silently drops owner/repo for non-GitHub origins), 449, 517–520, and 1967 when building run manifests from a local checkout. The plan declares the manifest lane (full `git.origin_url`) the *primary entry* for Forgejo runs, yet never audits the crate that constructs those manifests. A local `fabro run` in a Forgejo checkout may produce a manifest with the origin but missing the owner/repo identity downstream PR creation needs. Fix is the same dispatch pattern as every other site — additive, test-catchable, no decision reversal.
  > 
  > **2. Two threading links are unnamed.** `run_manifest.rs:602` (`parse_github_owner_repo` in the git-coordinate path, before preflight) and `fabro-workflow/src/operations/start.rs:114` (`RunServices.github_app`, which feeds `publish.rs`). The plan names `run_options.rs`, `services.rs`, `publish.rs`, and says "thread into worker run options," so the chain is implied — but these two specific links are where a Forgejo run would fail first. Greppable in minutes.
  > 
  > **3. The Gitea→`PullRequestGithubDetail` DTO mapping is never called out.** Forgejo's PR JSON is not GitHub-shaped (different `state` vocabulary, nested head/base objects). `get_pull_request` must convert into the shared `PullRequestGithubDetail`. The plan's wording ("handlers branch identically") implies it, but the conversion is the riskiest parsing code in the crate and deserves a named test fixture.
  > 
  > **4. Twin-forgejo inherits my API assumptions.** Both the mock tests and the twin encode the same possibly-wrong beliefs about the Gitea-lineage REST surface (merge endpoint parameters, auto-merge shape, token-in-URL username). Mock tests would pass even if the real API differs. Mitigations exist in the plan (explicit auto-merge fallback, env-gated live tests, mandatory manual E2E against a real Forgejo docker) — but the live tests are opt-in, so the manual pass is the real gate and must not be skipped.
  > 
  > **5. Web run-creation path unverified.** PR chips are `html_url`-driven (fine), but nothing in Phase 5 confirms the web/playground run-creation form passes a Forgejo URL through the manifest lane rather than validating for github.com. If it validates, a small addition is needed; the plan says "verify" for chips but not for creation input.
  > 
  > **6. Rollback nuance:** adding `[server.integrations.forgejo]` to settings.toml means downgrading to an older binary will reject the section (settings structs are strict). Standard posture for this repo (new keys always do this), not a plan defect.
  > 
  > ## Bar check
  > 
  > None of these forces redoing work or reverses a decision: the architecture (additive crate, single instance, PAT, dispatch points) is unaffected; every gap is an additive call-site audit or a named test within files/patterns the plan already establishes, and the scenario test in Phase 6 would surface each one. Per the stated bar — blocking means wrong work or expensive-to-reverse decisions — these are implementer judgment calls plus review-stage catches, not revision triggers. The `PullRequestLink.forge` wire addition is explicitly flagged, justified, and cheap to remove if unwelcome.
  > 
  > The plan is sound enough to build from, with the six notes above carried into implementation.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > The threading hop is confirmed: plain struct fields (`github_app: Option<GitHubCredentials>` in `StartOptions`/`StartServices`), `.clone()` at each of five hops, consumed directly by `publish.rs`. That grounds the decision.
  > 
  > ## Reasoning
  > 
  > **1. The central refactoring question was already answered by the human.** The vetted architecture question offered exactly this choice: additive parallel crate vs. shared `Forge` trait refactor across ~100 call sites. The answer was additive, with "GitHub code paths untouched" as the stated benefit. A refactoring stage that extracted a forge abstraction would override an authoritative answer — the main candidate is disqualified before any code reading.
  > 
  > **2. The seams this change needs are part of the change, not pre-work.** `PullRequestHost`, `CloneDecision::Forge`, `ForgejoInstance`, the parallel credential fields — none can exist meaningfully before the `fabro-forgejo` crate and its config types do. They are the plan's own Phase 0–2 content. Doing them "first" as a refactor would just be doing the implementation in the wrong order.
  > 
  > **3. The parallel-field threading is mechanical, not the plan working around the code.** Just verified in `operations/start.rs` and `publish.rs`: each hop is one struct field plus a `.clone()`. The actual dispatch logic is confined to two consumption points (`publish.rs` host selection, `initialize.rs` token injection). Five additive fields is the accepted price of zero GitHub regression risk, and the diff stays proportionate to the idea.
  > 
  > **4. Duplication is bounded and was explicitly accepted.** The `HttpClient`/`HttpMethod`/`HttpResponse` trio and `MockHttpClient` will be duplicated into `fabro-forgejo` (~150 lines). The human saw "some duplicated shape to maintain" in the option they picked. Deduplicating it into `fabro-http` would touch `fabro-github`'s public surface — against the answer — and only becomes clearly worthwhile when a third forge arrives. On record as a future item, not this one.
  > 
  > **5. No test is blocked by the current shape.** Mock-client unit tests, a twin-forgejo harness (the twin-github pattern exists to copy), spec/router conformance tests, and CLI scenario tests all reach the new behaviour with existing tooling.
  > 
  > **6. Names stay honest.** The plan already renames `GitHubRepoLayout` → `CloneRepoLayout`; `GitHubRepositorySlug` remains accurate for its github.com-only remaining uses; the automation lane legitimately stays GitHub-only per the scoping answer.
  > 
  > Refactorings considered and rejected as unrelated-to-this-change (on record): extracting the HTTP trio to `fabro-http`; splitting `pipeline/pull_request.rs` (2,046 lines) and `server.rs` into smaller modules; the forge-host trait (human-rejected — if ever revived it needs its own goal and conversation).
  > 
  > No refactoring stage has run in this workflow; this is the first and only decision point, and the answer is no.
  > 
  > {"preferred_next_label": "none"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/test/twin/forgejo/tests/client_ops.rs
- Response:
  > Everything is verified. The Daytona and Docker clone paths both consume the decision's `origin_url`, which now carries the port correctly through `embed_token_in_url`. Here's the complete summary.
  > 
  > ## Summary
  > 
  > The deep_review stage found one blocking defect: **port-bearing Forgejo instances (including Forgejo's default port 3000) failed everywhere** because `normalize_https_host_path` swallowed the port into the path, and three call sites pre-normalized origins with the GitHub normalizer — which contains the identical mangle — before the forge check. I implemented the review's fix point by point, plus one adjacent same-class site found while tracing.
  > 
  > ### Files changed
  > 
  > **`lib/components/fabro-forgejo/src/lib.rs`** — the core fix (review point 1): `normalize_https_host_path` is now port-aware. When the segment between `:` and the first `/` is all ASCII digits, it's treated as an instance port and the URL passes through untouched; only the sanitized scp-like `host:owner/repo` shape (non-numeric segment) is rewritten to `host/owner/repo`.
  > 
  > **`lib/components/fabro-sandbox/src/clone_source.rs`** — review point 2:
  > - `parse_origin_owner_repo` now runs `is_forgejo_origin` on the **raw** origin (it normalizes internally) and dispatches to `parse_forgejo_owner_repo` there; the GitHub branch is unchanged, still parsing the GitHub-normalized URL.
  > - `decide_clone` likewise dispatches on the raw origin, then stores each branch's own canonical form: `normalize_forgejo_origin_url` (port-preserving) for `CloneDecision::Forge`, the GitHub normalizer for `CloneDecision::GitHub` — so the clone URL both Docker and Daytona consume is correct.
  > - `clean_clone_origin_for_record` — **same defect class, not on the review's list**: it GitHub-normalized unconditionally for the persisted sandbox record, so a port-bearing Forgejo origin would be recorded (and later shown on reconnect/inspect) as `https://host/3000/owner/repo`. It now takes the configured instance and dispatches normalizers exactly like the other sites. Both `sandbox_spec.rs` call sites updated.
  > 
  > **`lib/apps/fabro-server/src/run_manifest.rs`** — review point 2: `run_repository_access_check_with` runs the forge check on the raw `git.origin_url`; the forge branch reports the forge-normalized origin (credential-stripped, port-preserving) in its details, and the GitHub branch below is byte-identical to before.
  > 
  > **Tests** (review point 3, plus one extra):
  > - `fabro-forgejo/src/tests.rs` — 4 new tests: port-bearing instance keeps port/host/api-base; normalizer preserves the port for the `https://`, `ssh://`, and credential-bearing forms; `is_forgejo_origin` matches its own port-bearing origins only (sc-style URLs legitimately never match); `parse_forgejo_owner_repo` on a port-bearing instance.
  > - `clone_source.rs` tests — 3 new: `decide_clone` → `CloneDecision::Forge` with port, `clone_repo_layout` forge parse with a credential-bearing port URL, and record normalization keeping the port.
  > - `run_manifest.rs` tests — 1 new: a port-bearing origin with a matching instance dispatches to the forge preflight branch (observable via its distinct remediation, using an instant-refusal loopback port), not the supported-origins error; the GitHub remote probe is never invoked.
  > - `test/twin/forgejo/tests/client_ops.rs` — 1 new end-to-end test: the twin's inherently port-bearing `http://127.0.0.1:<port>` instance URL flows through `is_forgejo_origin`/`parse_forgejo_owner_repo` ��� this closes the review's note that twin tests never passed an origin URL through the normalizer.
  > 
  > ### Adaptations and deviations
  > 
  > - **`clean_clone_origin_for_record`** was fixed beyond the review's named sites — I believe this is in the spirit of the finding ("port-bearing instances do not work at all"): it was the persisted-record leg of the same root cause, and leaving it would surface corrupted origins in `RunSandboxRuntime` metadata.
  > - **`preflight_sandbox_spec`'s** GitHub pre-normalization (line ~1039) was left unchanged deliberately: preflight sets `skip_clone = true`, so `decide_clone` short-circuits to `EmptyWorkspace` before ever inspecting the origin — verified inert for ports, and the review's instruction was to keep the GitHub branch exactly as today.
  > - **One pre-existing, unrelated issue left alone:** `install::tests::write_artifact_store_metadata_*` and two `serve::tests` object-store tests fail only in full parallel `cargo test -p fabro-server` runs — confirmed failing identically on the base commit (they pass in isolation). Test-isolation flakiness, out of scope for this fix.
  > 
  > ### Validation
  > 
  > - Full `cargo build --workspace` clean; `cargo +nightly-2026-04-14 fmt --check --all` clean; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings` clean.
  > - Tests: fabro-forgejo 35/35, fabro-sandbox (docker) 218/218, fabro-server 910 + the new dispatch test (the 3 flaky failures are the pre-existing ones above), fabro-workflow 1445/1445, twin-forgejo 18/18, and a full workspace run with zero failures.

## Stage: fork
- Status: succeeded
- Handler: parallel
- Notes: Parallel node dispatched 4 branches (4 succeeded, 0 failed)

## Stage: synth
- Status: succeeded
- Handler: parallel.fan_in
- Notes: Stage completed: synth

## Stage: deep_review
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > ## Review complete
  > 
  > I reviewed the change directly rather than ratifying: read the new crate in full, the webhook/PR-handler/install/clone/preflight/git-bridge diffs with surrounding code, checked the wire-format and event backward compatibility, and ran the actual test suites.
  > 
  > **Verified sound:**
  > - **Build & tests:** workspace builds clean; fabro-forgejo (31), twin-forgejo (17), fabro-sandbox (133), fabro-types (465+), fabro-server forge/webhook handlers (9), fabro-workflow (1445), fabro-api round-trips, CLI `pr_link` (4), and OpenAPI conformance (`all_spec_routes_are_routable`) all pass. An earlier CLI link failure was a transient environment linker error; clean on rerun.
  > - **Security:** webhook HMAC is constant-time, accepts the three real header spellings, inert by decision, auth-slot gated identically to GitHub. PAT never appears in `Debug`, is redacted via `DisplaySafeUrl`, scrubbed through the secret registry, validate-before-persist at install, URL-only in settings.toml, vault-only token. A test asserts no token in bridge env values.
  > - **Compatibility:** `PullRequestLink.forge` is omitted for GitHub links (byte-identical wire), serde-defaulted on read with a host+port-checked `html_url` cross-validation; events/store changes additive; GitHub code paths in `fabro-github` are untouched (additive, per the human's architecture answer); scope matches every answered question (single instance, PAT-only, inert webhooks, install flow, diagnostics, twin, docs/changelog).
  > 
  > **One blocking defect, confirmed empirically** (I built a scratch binary against the crate):
  > 
  > Port-bearing Forgejo instances — including Forgejo's own default port 3000 — do not work at all:
  > 
  > 1. `fabro-forgejo/src/lib.rs::normalize_https_host_path` (lines 402–413) rewrites `https://host:3000/owner/repo` to `https://host/3000/owner/repo`, swallowing the port into the path. `is_forgejo_origin("https://git.example.com:3000", "https://git.example.com:3000/acme/widgets.git")` returns `false`; `parse_forgejo_owner_repo` errors. Every downstream path (clone admission, preflight, PR host selection, token gating) funnels through these two functions.
  > 2. The bug is compounded upstream: `clone_source.rs` (`decide_clone` ~line 344, `parse_origin_owner_repo` ~line 72) and `run_manifest.rs` (`run_repository_access_check_with` ~line 850) pre-normalize origins with `fabro_github::normalize_repo_origin_url` — which contains the identical port-mangling helper — *before* the forge check. Fixing only the forgejo crate leaves sandbox cloning and preflight broken for ports.
  > 3. No test covers it: all fixtures use port-less `git.example.com`, and the twin tests build contexts from owner/repo constants, never passing an origin URL through the normalizer. The codebase is even internally inconsistent — `PullRequestLink::from_forgejo_url` compares instance ports correctly.
  > 
  > **Fix (precise):**
  > - Make `normalize_https_host_path` port-aware in `fabro-forgejo/src/lib.rs`: when the segment between `:` and the first `/` is all ASCII digits, treat it as a port and leave the URL unchanged; only the sanitized `host:owner/repo` shape (non-numeric segment) should be rewritten.
  > - In `clone_source.rs::parse_origin_owner_repo`/`decide_clone` and `run_manifest.rs::run_repository_access_check_with`, run the forge check on the raw origin (`is_forgejo_origin` normalizes internally) rather than on the GitHub-normalized URL, so the GitHub normalizer's mangling can't corrupt it first. The GitHub branch stays exactly as today.
  > - Add tests: instance `https://git.example.com:3000` against `https://…:3000/acme/widgets.git`, `ssh://git@…:3000/…`, and a credential-bearing form; a `decide_clone` → `CloneDecision::Forge` case; a `clone_repo_layout` forge case. (Scp-style `git@host:owner/repo` without a port legitimately cannot match a port-bearing instance — no change needed.)
  > 
  > **Non-blocking notes:** `bun`, pinned nightly clippy, and nextest are unavailable in this environment, so web tests and clippy weren't independently rerun here (implement stage reported success; CI gates both). The twin encodes the same Gitea-API assumptions as the client, so the plan's live-test/manual E2E gate against a real instance remains the real proof of the REST shapes.
  > 
  > {"preferred_next_label": "fix"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/test/twin/forgejo/tests/client_ops.rs
- Response:
  > Everything is verified. The Daytona and Docker clone paths both consume the decision's `origin_url`, which now carries the port correctly through `embed_token_in_url`. Here's the complete summary.
  > 
  > ## Summary
  > 
  > The deep_review stage found one blocking defect: **port-bearing Forgejo instances (including Forgejo's default port 3000) failed everywhere** because `normalize_https_host_path` swallowed the port into the path, and three call sites pre-normalized origins with the GitHub normalizer — which contains the identical mangle — before the forge check. I implemented the review's fix point by point, plus one adjacent same-class site found while tracing.
  > 
  > ### Files changed
  > 
  > **`lib/components/fabro-forgejo/src/lib.rs`** — the core fix (review point 1): `normalize_https_host_path` is now port-aware. When the segment between `:` and the first `/` is all ASCII digits, it's treated as an instance port and the URL passes through untouched; only the sanitized scp-like `host:owner/repo` shape (non-numeric segment) is rewritten to `host/owner/repo`.
  > 
  > **`lib/components/fabro-sandbox/src/clone_source.rs`** — review point 2:
  > - `parse_origin_owner_repo` now runs `is_forgejo_origin` on the **raw** origin (it normalizes internally) and dispatches to `parse_forgejo_owner_repo` there; the GitHub branch is unchanged, still parsing the GitHub-normalized URL.
  > - `decide_clone` likewise dispatches on the raw origin, then stores each branch's own canonical form: `normalize_forgejo_origin_url` (port-preserving) for `CloneDecision::Forge`, the GitHub normalizer for `CloneDecision::GitHub` — so the clone URL both Docker and Daytona consume is correct.
  > - `clean_clone_origin_for_record` — **same defect class, not on the review's list**: it GitHub-normalized unconditionally for the persisted sandbox record, so a port-bearing Forgejo origin would be recorded (and later shown on reconnect/inspect) as `https://host/3000/owner/repo`. It now takes the configured instance and dispatches normalizers exactly like the other sites. Both `sandbox_spec.rs` call sites updated.
  > 
  > **`lib/apps/fabro-server/src/run_manifest.rs`** — review point 2: `run_repository_access_check_with` runs the forge check on the raw `git.origin_url`; the forge branch reports the forge-normalized origin (credential-stripped, port-preserving) in its details, and the GitHub branch below is byte-identical to before.
  > 
  > **Tests** (review point 3, plus one extra):
  > - `fabro-forgejo/src/tests.rs` — 4 new tests: port-bearing instance keeps port/host/api-base; normalizer preserves the port for the `https://`, `ssh://`, and credential-bearing forms; `is_forgejo_origin` matches its own port-bearing origins only (sc-style URLs legitimately never match); `parse_forgejo_owner_repo` on a port-bearing instance.
  > - `clone_source.rs` tests — 3 new: `decide_clone` → `CloneDecision::Forge` with port, `clone_repo_layout` forge parse with a credential-bearing port URL, and record normalization keeping the port.
  > - `run_manifest.rs` tests — 1 new: a port-bearing origin with a matching instance dispatches to the forge preflight branch (observable via its distinct remediation, using an instant-refusal loopback port), not the supported-origins error; the GitHub remote probe is never invoked.
  > - `test/twin/forgejo/tests/client_ops.rs` — 1 new end-to-end test: the twin's inherently port-bearing `http://127.0.0.1:<port>` instance URL flows through `is_forgejo_origin`/`parse_forgejo_owner_repo` ��� this closes the review's note that twin tests never passed an origin URL through the normalizer.
  > 
  > ### Adaptations and deviations
  > 
  > - **`clean_clone_origin_for_record`** was fixed beyond the review's named sites — I believe this is in the spirit of the finding ("port-bearing instances do not work at all"): it was the persisted-record leg of the same root cause, and leaving it would surface corrupted origins in `RunSandboxRuntime` metadata.
  > - **`preflight_sandbox_spec`'s** GitHub pre-normalization (line ~1039) was left unchanged deliberately: preflight sets `skip_clone = true`, so `decide_clone` short-circuits to `EmptyWorkspace` before ever inspecting the origin — verified inert for ports, and the review's instruction was to keep the GitHub branch exactly as today.
  > - **One pre-existing, unrelated issue left alone:** `install::tests::write_artifact_store_metadata_*` and two `serve::tests` object-store tests fail only in full parallel `cargo test -p fabro-server` runs — confirmed failing identically on the base commit (they pass in isolation). Test-isolation flakiness, out of scope for this fix.
  > 
  > ### Validation
  > 
  > - Full `cargo build --workspace` clean; `cargo +nightly-2026-04-14 fmt --check --all` clean; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings` clean.
  > - Tests: fabro-forgejo 35/35, fabro-sandbox (docker) 218/218, fabro-server 910 + the new dispatch test (the 3 flaky failures are the pre-existing ones above), fabro-workflow 1445/1445, twin-forgejo 18/18, and a full workspace run with zero failures.

## Current context
| Key | Value |
|-----|-------|
| parallel.branch_count | 4 |
| parallel.results | 23.9 KB; full value: `/tmp/fabro/runtime/blobs/5ea0a594a8da0438cb68f865e5ee350400f8c51cce03e410f58d38dd900dfa55.json`; Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. Here is my correctness assessment of the Forgejo integration (127 files, ~8.4k insertions).\n\n## What I verified as correct\n\n- **Webhook HMAC** (`forgejo_webhooks.rs`): constant-time `","last_st… |


**First, get the change in front of you.** Nothing has been handed to you, so
find it yourself:

    git --no-pager diff origin/main...HEAD

If `origin/main` does not resolve, use `main` instead. Read whole files with
`git show` or by opening them whenever the diff alone is not enough to judge —
a diff hides the context a line sits in, and most real defects live in that
context.

You are reviewing only. Do not edit, create, or delete any file, and do not
commit. Three other reviewers are working in this same checkout at the same
time, and a write from you would corrupt what they are reading.

Review the change for correctness only.

Look for logic that produces a wrong result: off-by-one errors, inverted
conditions, unhandled nil or empty cases, incorrect operator precedence, wrong
variable used, race conditions, resource leaks.

For each finding give the file, the line if you can identify it, what is wrong,
and the concrete input or state that triggers it. A finding you cannot make
fail is a guess — label it as such or drop it.

If the diff is correct, say so plainly. Do not invent findings to seem useful.
