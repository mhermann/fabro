Deep pass complete. Below is the deliberate sweep, the bucket sort, the stress-test, and the final question set.

# Dimension-by-dimension sweep

**1. Scope boundaries.**
Clearly in: a fourth provider kind wired through the whole stack (enums, `Sandbox` + `SandboxProvider` impls, config mapping, policy, API/UI, docs), following the clone-based contract. Clearly out: changing Local/Docker/Daytona behavior; changing the `Sandbox` trait's shape; running the Fabro server itself on K8s (docs-only today). Unclear: how much capability parity the first cut must have (terminal? preview URLs? dockerfile builds? network enforcement?) — this is where two readings produce materially different software, and the repo does *not* settle it: history shows providers accrete scope over dozens of commits (Daytona: 56 commits since introduction), but a single run still needs a boundary.

**2. Users and callers.**
Callers: the run worker (`fabro-workflow/src/operations/start.rs` builds `SandboxSpec` — runs in a spawned worker process with `env_clear()` + strict env allowlist, `fabro-server/src/spawn_env.rs`), server preflight (`run_manifest.rs`), inventory handlers (`handler/sandboxes.rs`), reconnect/terminal/file endpoints (`server.rs`, `run_files.rs`, `handler/sandbox.rs`), the web UI (`environment-providers.ts` `CREATABLE_PROVIDERS`), and `fabro-install` (Docker/Daytona selection). Wire surfaces: `SandboxProviderKind` and `EnvironmentProvider` enums in the OpenAPI spec (additive), `ServerSandboxProvidersSettings` (`required: [local, docker, daytona]` must grow). Existing callers' behavior must not change — all matches on the kind enum are exhaustive and get a new arm; Docker/Daytona arms stay untouched.

**3. Data and state.**
Persisted state: `RunSandboxInstance` records (provider string, runtime id, clone metadata) in the run store; server-managed environments (provider string) in SQLite; `[server.sandbox.providers]` in settings.toml. No migration of old data is implied — additive enum value plus a new optional settings field (resolver already defaults missing provider entries to enabled). Rollback: a run created with `provider = "kubernetes"` cannot be reconnected by an older binary (FromStr fails) — same additive-enum risk every variant addition carries; note it, nothing to build. Not ambiguous.

**4. Existing behavior.**
Verified, not assumed: `SandboxCreateSpec` (inventory `create`) has **zero production constructors** — only the provider impls destructure it; the registry's live surface is list/get/delete. `get_preview_url` has **no workflow-layer callers** — only the Daytona-gated server endpoints. Local has **no terminal session** (terminal.rs falls through to an error). Docker **ignores `auto_stop`** (no `set_autostop_interval` impl) and supports `stop_on_terminal` via container stop/start (filesystem survives); `preserve` skips delete (server.rs ~2741). The goal is pure extension; nothing existing changes semantics.

**5. Edge and failure cases.**
The provider contract already dictates most handling (bash probe, git retry, clone-failure classification, timeout/cancel, redaction, managed-label delete refusal, `SandboxLookupError` aggregation). K8s-specific gaps the goal says nothing about: pod eviction/node death mid-run (vs. Docker daemon restart — reconnect must handle a vanished pod), image pull failures/backoff, API-server unavailability during exec streaming, scheduler-unavailable pod Pending forever (needs a wait-with-timeout), workspace ephemerality on stop (pod delete loses the filesystem — Docker's stop preserves it; resume is git-checkpoint-based so this may be acceptable, but it's a semantic choice), namespace full (quota). Mostly mine to handle following Docker's error taxonomy; workspace ephemerality is noted in Decisions.

**6. Non-functional constraints.**
Pod cold-start + image pull latency exceeds Docker container start; nothing in the repo states latency budgets. `server.scheduler.max_concurrent_runs` already bounds concurrency; a namespace quota is the operator's analogue. No stated perf expectations to violate. Not a question.

**7. Security, privacy, permissions.**
This widens where code executes: from "operator's Docker daemon (documented host-root-equivalent, trusted single-tenant)" to "a cluster the server/worker authenticates to." Key deltas: what RBAC the Fabro service needs (pods exec if direct API vs. just pod management with an in-pod agent); GitHub installation tokens flow into pods via exec env exactly as Docker does today; any kubeconfig/token must be a registered secret (`fabro-static` secret_registry + vault, never ambient env — the worker env allowlist is fail-closed and must stay so). The transport choice (Q1) *is* the security-posture choice.

**8. Compatibility and versioning.**
Additive wire enum values; embedded SPA ships with the binary so server+UI move together; TS client regenerates in-repo. Feature-gated behind a new `kubernetes` cargo feature (precedent: `docker`, `daytona`). No deprecation. No staged rollout machinery exists in the repo. Nothing to ask.

**9. Testing and verification.**
Repo convention answers this: unit tests for config mapping/validation/command-building require no cluster (Docker's 29 unit tests need no daemon); tests needing real sandboxes get `#[ignore]` with a reason or run env-gated (`testing-strategy.md` §"real providers"). No kind/k3d CI exists and adding it is a new infrastructure decision I won't make silently for CI cost reasons — but per convention, live-cluster tests are `#[ignore]`/env-gated locally. Decided; the only open bit (which cluster the human validates against) feeds Q5's CNI caveat.

**10. Operational surface.**
Conventions require: `docs/public/execution/environments.mdx` + `administration/sandboxing.mdx` updates, a dated `docs/public/changelog/*.mdx` entry, tracing per logging-strategy, errors via the crate's existing `Error` taxonomy (error-handling-strategy), no new `SandboxEvent` variants needed. Not a question.

**11. Dependencies and integration.**
New external dependency: a Kubernetes client (`kube` + `k8s-openapi` — the only maintained Rust client of note; bollard precedent exists for heavy clients). New infrastructure assumed: a cluster, an image registry reachable from that cluster, credentials. External-service shape (registry for dockerfile builds, cluster config) is scope-question material (Q2/Q4), not silently decidable.

**12. Definition of done.**
A run configured with a `kubernetes` environment executes a full workflow in a pod (clone, exec, files, git push, cleanup), appears correctly in UI/API inventory, respects provider policy, and is documented. The rejection risk — "every test passes but it's not what I asked for" — concentrates exactly in Q1–Q5 below: transport, image story, capability surface, cluster config, network guarantees.

# Bucket sort

**Answerable from the repository (answered):**
- Where the change lands and the exhaustive list of match sites (prior stage).
- Feature wiring: `fabro-server` enables `daytona,docker`; `fabro-cli`/`fabro-workflow` enable `daytona` only → a `kubernetes` feature follows this pattern.
- Inventory `create` has no production callers → low-risk parity completeness, not a hidden consumer.
- Preview URLs are Daytona-only extras, not workflow-critical → deferrable without breaking MCP.
- Local lacks terminal support → deferring terminal is not unprecedented.
- Docker ignores `auto_stop` → K8s may too.
- Credential transport to workers: vault via `FABRO_HOME` (spawn_env.rs documents this exact pattern for provider secrets); ambient `KUBECONFIG` cannot reach workers (env allowlist is fail-closed by test).
- Testing convention for cluster-dependent code (`#[ignore]`/env-gated; pure unit tests otherwise).
- `preserve` skips provider delete; `stop_on_terminal` defaults true (server.rs + finalize.rs).
- Providers default to enabled in the resolver; Daytona additionally gates registration on a vault key.

**Yours to decide (decided — see final section):** wire name, pod-per-run primitive, clone mechanics, resource mapping, default image, lifecycle semantics, registration gating, dependency choice, testing approach, rollback note, docs/changelog surface.

**Genuinely theirs:** execution transport, dockerfile/image story, capability parity boundary, cluster targeting/config model, network enforcement semantics — the five vetted questions.

# Stress-test of the question set

- **Regret check.** Imagined rejections: "you built a sidecar agent — I wanted plain kube exec" (Q1); "it rejects my Dockerfile environments — I need builds" (Q2); "the web terminal is dead for K8s runs — I assumed parity" (Q3); "we run Fabro in-cluster, your explicit-config requirement is wrong" / vice versa (Q4); "our CNI doesn't enforce NetworkPolicy and you silently advertised isolation" or the inverse (Q5). Also considered "did you mean a K8s-native operator/CRD?" — the `Sandbox` trait contract (persistent cwd, PTY, reconnect, synchronous exec) forecloses it; stated as a decision instead of a question. Nothing else survived this check.
- **Second-round check.** Predictable follow-ups: "which cluster do I test against / does your CNI enforce egress?" — folded into Q5 as context. "Should CI run kind?" — settled by repo convention (no). "kube-rs version?" — mine. No remaining second-round triggers.
- **Merge check.** Merged "runtime primitive" and "exec transport" into one Q1 (they are one architecture decision). Merged "where do credentials live" and "single vs multi cluster" into Q4. Considered merging Q3+Q5 as "capability scope" but rejected: Q5 is a security/validation contract with different failure modes than UX parity — different code layers, different wrongness.
- **Self-answer check.** Each candidate was tested against the repo: Q1 has precedent for *both* answers (Docker = direct client; Daytona = hosted API/agent), so the repo argues neither side. Q2: Daytona's snapshot machinery is platform-provided; K8s has none — unanswerable. Q3: repo history shows incremental growth but sets no boundary for a single deliverable. Q4: repo has both zero-config (Docker socket) and vault-key (Daytona) precedents. Q5: repo has both full enforcement (Daytona) and honest-rejection (Docker rejects CIDR lists; Local hard-errors) precedents. All five survive.
- **Consequence check.** Each question below states concretely what changes; any without a stated difference was cut (e.g., naming, default image, resource mapping — cut to Decisions).

## Vetted questions

1. **How should Fabro execute commands inside a Kubernetes sandbox?**
   - **A. Direct Kubernetes API (`kube-rs` exec/attach):** one pod per run, server+worker hold pod create/exec/delete RBAC. No new artifacts; RBAC posture is docker-sock-like (trusted single-tenant); stdin streaming/PTY ride the API-server WebSocket.
   - **B. In-pod agent sidecar:** Fabro publishes an agent image; sandbox talks HTTP/SSH to it (Daytona-toolbox-like). Simpler streaming/PTY and lighter API-server load, but a second release artifact + registry dependency, and pods must be able to pull it.
   - I build A vs. B differently in: dependency set, release pipeline (new agent image or not), RBAC docs, and the entire `exec_command_streaming`/`spawn_stdio_process`/terminal implementation.

2. **What must `image.docker` / `image.dockerfile` mean for Kubernetes environments?**
   - **A. Prebuilt image refs only:** `dockerfile` is rejected at config validation with a clear error; operators build elsewhere. Cheap, but existing dockerfile environments can't target K8s.
   - **B. Full support:** build from dockerfile and push to an operator-configured registry (new `[server.sandbox.kubernetes]` registry settings + credentials, plus a build mechanism). Significant machinery, Daytona-snapshot-like identity caching optional.
   - I build A vs. B differently in: config validation rules, presence/absence of registry settings + secrets, snapshot-identity machinery, and how much of Daytona's build lifecycle gets a K8s analogue.

3. **Which capability surface must the first cut ship?**
   - **A. Core clone-based parity:** full `Sandbox` trait (exec streaming, files, walk, git setup, push credentials), inventory list/get/delete, reconnect, config/policy/UI/API wiring, docs — but **no** PTY terminal, **no** preview URLs/VNC, **no** install-wizard entry (each returns an explicit "not supported" error, as Local does for terminal).
   - **B. A + interactive web terminal (PTY).** Adds the terminal transport on top of Q1's answer.
   - **C. Full parity** including port preview URLs (port-forward/ingress), VNC, install wizard.
   - I build A vs. B vs. C differently in: terminal.rs work, preview/port-forward plumbing, `fabro-install` changes, and roughly 1×/1.3×/2× the surface of A.

4. **How does an operator point Fabro at a cluster?**
   - **A. Ambient only:** in-cluster ServiceAccount or the server's kubeconfig (`KUBECONFIG`/`~/.kube/config`), zero new settings; workers receive credentials via vault, not ambient env.
   - **B. Explicit operator config:** `[server.sandbox.kubernetes]` in settings.toml (context, namespace, kubeconfig path-or-vault-secret), with ambient as fallback; single cluster per server.
   - **C. Per-environment cluster selection** (multi-cluster).
   - I build A vs. B vs. C differently in: settings schema + resolver, vault secret registration, install diagnostics, and whether environment creation needs cluster fields.

5. **How should network modes (`block`, `cidr_allow_list`) be handled — enforce or honestly reject?**
   - **A. Enforce via per-sandbox `NetworkPolicy`** (default-deny egress for `block`; `ipBlock` rules for allow-lists), documented as requiring a policy-enforcing CNI (Calico/Cilium; plain flannel silently ignores policies).
   - **B. Reject both at validation** (allow_all only) until enforcement can be guaranteed — the Docker precedent (Docker rejects CIDR lists; Local hard-errors rather than pretending).
   - Which cluster will you validate against (EKS/GKE/kind/other) matters here: I build A vs. B differently in: validation rules in `fabro-config`, NetworkPolicy create/delete lifecycle alongside the pod, and the docs' security claims.

## Decided without asking

- **Wire/provider name: `kubernetes`** (spelled out; matches Local/Docker/Daytona single-word variants; `k8s` is a nickname and strum lowercase handles the string form).
- **Runtime primitive: one long-running pod per run sandbox** (entrypoint overridden to idle), not Jobs, not a CRD/operator — forced by the `Sandbox` contract (persistent cwd, `spawn_stdio_process`, PTY, reconnect-by-id).
- **Clone-based, Docker-style:** extend `is_clone_based()`; exec-based clone inside the pod reusing `git_retry`/push-credential machinery; `/workspace` + `/repos` layout constants like Docker.
- **Default image: `buildpack-deps:noble`** (same as Docker; satisfies the bash+git contract).
- **Resource mapping:** cpu/memory → pod requests=limits (Guaranteed QoS, matches "dedicated sandbox" semantics); disk → ephemeral-storage limit; no PVCs initially.
- **Lifecycle:** `stop_on_terminal` → delete pod (workspace is ephemeral; run state lives in git checkpoints — unlike Docker, an uncommitted filesystem does not survive stop; will be documented); `preserve` → leave pod running; `auto_stop` ignored (Docker precedent, no idle detection).
- **Inventory safety:** reuse `managed_labels` on pod objects for list/get filtering and delete-refusal of unmanaged resources.
- **Registration gating:** register only when enabled **and** cluster access is actually configured (Daytona's key-gating pattern), so ambient clusters don't silently activate.
- **Credential transport:** vault-based (Daytona pattern); new env-var constants + secret-registry entries; the worker env allowlist stays fail-closed (no ambient `KUBECONFIG` pass-through).
- **Dependency:** `kube` + `k8s-openapi` behind a new `kubernetes` cargo feature in `fabro-sandbox`, enabled by `fabro-server` (and `fabro-cli`/`fabro-workflow` following the daytona pattern).
- **Testing:** pure unit tests (config mapping, validation, labels, command assembly — no cluster), live-cluster tests `#[ignore]`/env-gated per `testing-strategy.md`; no kind/k3d in CI (no such infra exists in-repo).
- **Compat note (no action):** runs persisted with `provider: "kubernetes"` are unresolvable by older binaries — inherent to additive enum values, same as every prior variant.
- **Docs/ops:** update `environments.mdx` + `sandboxing.mdx`, add a dated changelog entry, follow logging/error strategies; no new `SandboxEvent` variants.