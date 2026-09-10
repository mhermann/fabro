I've now read the remaining surfaces I needed: provider-capability validation, install-wizard handling, the `Sandbox` trait's required-vs-defaulted methods, worker resume/reconnect flow, Daytona's streaming-exec transport, diagnostics, the CLI/server dependency graph (which determines feature unification in the shipped binary), CI/release build flags, and the testing-strategy doc. Here is the full sweep.

---

## The sweep, dimension by dimension

### 1. Scope boundaries

**Clearly in (forced by the compiler and by repo vocabulary):** a fourth `SandboxProviderKind` — call it `kubernetes` — with (a) a `KubernetesSandbox: impl Sandbox` built from a new `SandboxSpec::Kubernetes` variant, (b) a `KubernetesSandboxProvider: impl SandboxProvider` for inventory, (c) threading through `EnvironmentProvider`, server provider policy, config parsing/validation, run-start, preflight, reconnect, details, OpenAPI + TS client, and the settings-sandboxes web page, behind a `kubernetes` cargo feature.

**Clearly out (by convention):** replacing or altering Docker/Daytona, changing the `Sandbox` trait contract, changing run admission, multi-tenant cluster orchestration.

**Genuinely unclear — with material divergence:**
- **Framing itself.** "A provider for running sandboxes in Kubernetes" maps onto the repo's term of art (`SandboxProviderKind`), but a second reading exists: "make Fabro deployable on Kubernetes" (server-in-cluster, still using local/docker sandboxes). The vocabulary strongly favors the provider reading (~95%), but the alternative is a completely different deliverable — the most expensive possible miss.
- **Execution primitive.** Pod-per-run is the only design that satisfies `exec_command_streaming` (mandatory override for production providers per its doc comment) plus start/stop/resume. Jobs/PVC-backed designs diverge materially — but the constraint pins it; I classify this as confirm-within-Q1 rather than open.
- **v1 interactive surface.** Embedded terminals (Daytona has one; requires the k8s v4 WebSocket exec protocol with resize), preview URLs (currently Daytona-only; would need an ingress story that doesn't exist), install-wizard integration (`InstallSandboxInput` enum is `[docker, daytona]`), and dockerfile support (k8s pulls but never builds). Each inclusion is real cost and real product surface; the goal says nothing.
- **Network enforcement depth.** Resolved below (decided).

### 2. Users and callers

Callers: server workers (`fabro-workflow` build path + `pipeline/initialize.rs` resume path), in-process CLI runs, `/api/v1/sandboxes` API consumers, the web settings page, the install wizard, `diagnostics.rs` health checks, demo mode. Wire formats touched: `SandboxProviderKind` and `EnvironmentProvider` in the OpenAPI spec (4 enum sites), generated TS client, and — important — **persisted run records** (`RunSandboxInstance`) and run events. Existing behavior must not change: default provider, dry-run coercion (`effective_for` maps non-local → Local; kubernetes joins that set), provider policy default `enabled = true`. Nothing here is ambiguous; the Rust compiler enumerates every exhaustive `match` that must gain an arm. → *Answerable, no question.*

### 3. Data and state

No migration implied: new enum variants deserialize old data fine; the new `ServerSandboxProvidersSettings.kubernetes` field follows the resolver-defaults pattern (missing `settings.toml` key → `enabled = true`, mirroring daytona). Cluster-side state = pods (+ optional NetworkPolicies) labeled `sh.fabro.managed=true`, deletable by label. Rollback within a version is trivial. **Downgrade after upgrade** (old binary reading `provider: "kubernetes"` run records) fails to deserialize — but this is exactly the property Daytona's addition had; accepted precedent, changelog note. → *Answerable, no question.*

### 4. Existing behavior

Additive everywhere. `is_clone_based()` gains `Kubernetes` (it will clone GitHub origins, so it must be in that set for the `RunTarget::Git` validation to accept it). Clone machinery (`clone_source`, `git_retry`, `push_credentials`, exact-SHA checkout) is provider-shared and reached via `exec_command` — a k8s provider that clones via exec inherits all of it for free. → *Answerable.*

### 5. Edge and failure cases

The goal is silent on all of these; each is designable from precedent: image pull failure / `ErrImagePull` → `InitializeFailed` with remediation (Daytona polls to Active with terminal-failure detection — copy that pattern for pod `Pending`/`ContainerCreating` with a pull-timeout allowance); pod evicted / node loss mid-run → resume/reconnect must produce a clean "sandbox is gone" error; API server unreachable → fail-soft inventory aggregation already handles it; quota/RBAC denial → first-use error with remediation text; concurrent runs → unique pod names + run-id labels (existing scheme). Not questions — failure-mapping work.

### 6. Non-functional constraints

No stated SLOs in the repo. Pod scheduling adds seconds vs Docker's container start — an operator concern, documented. The one real NFR is **dependency weight**: `kube-rs` + `k8s-openapi` is a heavy tree, and this repo has an explicit culture of avoiding heavy SDKs (the `aws-sdk-bedrockruntime` note in `Cargo.toml`; a hand-rolled AWS signing stack). However: streaming exec is *mandatory* for production providers, which forces either kube-rs (which bundles the WebSocket exec machinery; `tokio-tungstenite` is already in-tree via Daytona) or hand-rolling the SPDY/WS v4 subprotocol — weeks of high-risk work for a strictly worse outcome. → *Decided: kube-rs. Not a question.*

### 7. Security, privacy, permissions

This widens the trust boundary: the server gains kube API credentials. The repo's stated posture is trusted-single-tenant (the Docker-socket model), so a single namespace with pods CRUD + exec + networkpolicies RBAC matches. Connection model: standard kube inference (in-cluster ServiceAccount → `KUBECONFIG`/`~/.kube/config`) with explicit overrides in `[server.sandbox.providers.kubernetes]` — covers both a fabro-in-cluster deployment and the packaged compose deployment pointing at a remote cluster. Kubeconfig is a file, not a vault string (Daytona's API-key-in-vault pattern doesn't fit). No personal-data flow changes. Preview URLs skipped in v1 (no ingress/auth story — exposing them would be the security mistake). → *Decided.*

### 8. Compatibility and versioning

Feature-gated `kubernetes` cargo feature, default-off at the crate level, enabled by dependents. Pattern discovered from `Cargo.toml`/release.yml: the shipped binary is `cargo zigbuild -p fabro-cli`, and fabro-cli depends on fabro-server as a normal dependency, so anything fabro-server enables is unified into the shipped binary — enabling on fabro-server (+ fabro-cli, mirroring daytona's belt-and-braces placement) is sufficient and matches precedent. Runtime gating: register the provider only when cluster config resolves (mirrors `enabled && daytona_api_key.is_some()`). → *Decided.*

### 9. Testing and verification

The repo's testing strategy is explicit: unit tests beside code, `tests/it` for integration, httpmock for HTTP fakes, `#[ignore]`-gated live tests for real providers (docker_streaming.rs is exactly this and never runs in CI), twins only for LLM APIs. Plan: unit tests for pod-spec/label/name/network-policy generation; httpmock-backed REST tests (pods CRUD is plain JSON — easy to fake); live `#[ignore]` tests documented against a kind cluster for exec streaming/clone/push. A standalone kube twin would be a new artifact class the strategy reserves for LLM providers — not justified here. WebSocket exec can't be httpmocked; those paths go live-gated, same as Docker's. → *Decided.*

### 10. Operational surface

Convention-mandated and cheap: `SandboxEvent` tracing with `provider: "kubernetes"`, a diagnostics check card (Docker and Daytona each have one), docs updates (`sandboxing.mdx`, `environments.mdx`; `cargo dev docs check` runs in CI), a changelog entry (dated mdx file), and the settings-sandboxes web card. Demo-mode entry optional — minor. → *Decided: include all; they're table stakes here.*

### 11. Dependencies and integration

New: `kube` + `k8s-openapi` (version feature pinned, added to workspace deps). External: a cluster the operator supplies. Config: settings-file overrides, no forced env vars, no vault secret. Registry auth via optional `imagePullSecrets` list. → *Decided.*

### 12. Definition of done

"That is not what I asked for" scenarios, ranked: (1) built a fabro-on-k8s deployment story when a sandbox provider was wanted, or vice versa; (2) delivered run-execution + inventory but no terminal/install-wizard the human assumed was included; (3) named or configured it in a way that mismatches their cluster topology (covered by the kube-standard inference decision); (4) rejected `network.block` when they expected enforcement (covered by the NetworkPolicy decision). Only (1) and (2) survive as questions.

---

## Bucketing

**Answerable from the repository** (answers written above): every exhaustive-match site is compiler-enumerated; clone/git-push machinery is provider-shared via exec; resume flow shape (`pipeline/initialize.rs` reconnect + vault key); preflight shape (`preflight_sandbox_spec`); capability-validation precedent (Local rejects CIDR; Daytona rejects image+dockerfile — Kubernetes gets its own rule set); install-wizard shape; diagnostics/demo/docs/changelog conventions; testing conventions; feature-unification mechanics of the shipped binary; downgrade-compat precedent from Daytona's addition; single-tenant product posture (answers the multi-namespace question — server-level namespace).

**Yours to decide** (decisions in the next section): kube-rs as client; pod-per-run primitive; exec-based clone (not initContainer) to inherit all shared git machinery; provider string `kubernetes`; `image.dockerfile` rejected with a validation error; NetworkPolicy-based `block` and `cidr_allow_list`; `resources.disk` → ephemeral-storage limit; `stop()` deletes the pod; `set_autostop_interval` no-op (Docker precedent); image default `buildpack-deps:noble` (Docker parity); registration gated on resolvable cluster config; feature enabled on fabro-server + fabro-cli; skip preview URLs and VNC; httpmock + live-gated tests.

**Genuinely theirs:** the framing confirmation, and the v1 interactive-surface tier. Two questions.

---

## Stress tests

- **Regret check:** The rejection I can't recover from is the framing miss — added as Q1. The second-worst is surface mismatch (terminal/wizard assumed, missing) — Q2. A third candidate — "expected NetworkPolicy enforcement / expected it rejected" — fails the undo-cost test (flipping a validation rule is cheap), so it stays decided.
- **Second-round check:** Named every mid-build surprise I could (cluster version targeting, SPDY-vs-WS, PSA labels, pull-secret handling, pod restartPolicy, autostop, quota surfacing) — each resolved by precedent or cheap-to-flip decisions above. No second round required.
- **Merge check:** Framing + primitive merged into one Q1 (the primitive is implied by the framing). Terminal + wizard + dockerfile + docs merged into one tiered Q2. Client-library, namespace, and naming questions all dissolved into decisions.
- **Self-answer check:** Namespace topology (repo posture answers it), client library (mandatory streaming exec answers it), naming (obvious), testing depth (strategy doc answers it) — all removed from the question set.
- **Consequence check:** Q1 — a "deployment story" answer produces zero sandbox code and a Helm chart instead; a "provider" answer produces exactly the plan above. Q2 — minimal tier ≈ provider core + inventory + docs; interactive tier adds the WS terminal and wizard wiring, roughly 30–40% more surface. Both pass.

---

## Vetted questions

**Q1 — Confirm the deliverable's shape.** Is this a fourth sandbox provider (a new `SandboxProviderKind::Kubernetes` that creates and execs into one Pod per run, in a cluster the Fabro server reaches via kubeconfig or in-cluster ServiceAccount) — or did you mean a "run Fabro itself on Kubernetes" deployment story (Helm chart / manifests for the server, sandboxes unchanged)?
- **A. Fourth sandbox provider, Pod-per-run** *(my strong recommendation; matches the repo's "sandbox provider" vocabulary)* — builds the plan in this document.
- **B. Fabro-on-Kubernetes deployment packaging** — no sandbox code; compose equivalents, RBAC manifest, docs.
- **C. Both** — provider first, deployment packaging as a follow-up.
- *What changes:* A and B share almost no code; picking wrong wastes the entire run.

**Q2 — v1 interactive/product surface.** Beyond the core (create pod, clone, exec/streaming-exec, file ops, git push, stop/delete, inventory listing, diagnostics, docs), which tier?
- **A. Core only** — no embedded terminal, no install-wizard entry, `image.dockerfile` rejected with a clear validation error, no preview URLs. Cheapest; wizard stays `docker|daytona`.
- **B. Core + embedded terminal** (`kubectl exec`-style WebSocket terminal with resize, matching Daytona's UX) — adds the v4-channel exec session work and its live-gated tests.
- **C. Core + terminal + install-wizard tier** — `kubernetes` becomes a first-class install choice (API enum, wizard UI, connectivity check step), i.e., a supported-by-default deployment story.
- *What changes:* roughly 30–40% additional surface (WebSocket terminal session, install API/UI, changelog/docs positioning) between A and C; A is upward-compatible with B/C later.

## Decided without asking

- **Provider string `kubernetes`** (not `k8s`) — matches lowercase enum style; full word is the ecosystem norm.
- **Pod-per-run as the primitive** — the only object that supports `exec_command_streaming` (mandatory) plus start/stop/resume; Jobs complete and break exec.
- **kube-rs + k8s-openapi** as the client — streaming exec is mandatory, so hand-rolling the WS v4 subprotocol is strictly worse despite this repo's lean-dependency culture.
- **Clone via exec after pod start (Docker pattern), not an initContainer** — inherits `clone_source`, `git_retry`, `push_credentials`, exact-SHA checkout unchanged.
- **`stop()` deletes the pod** — k8s has no pause; Docker's pause semantic has no analog, and `preserve` skips stop anyway.
- **`set_autostop_interval` no-op** — Docker already no-ops it; only Daytona has native autostop.
- **Network: `block` and `cidr_allow_list` via per-run NetworkPolicy, `allow_all` default** — k8s natively supports ipBlock allow-lists (better than Docker, which rejects CIDR); CNI requirement documented.
- **`image.dockerfile` rejected for kubernetes environments** with a provider-capability validation error — k8s pulls but never builds; in-cluster builds (Kaniko) are out of scope. Follows the Local-rejects-CIDR precedent.
- **`resources.disk` → ephemeral-storage limit; cpu/memory → requests=limits** — k8s-native fidelity; Docker ignores disk, but k8s has a real knob.
- **Default image `buildpack-deps:noble`** — Docker parity (bash+git present, satisfying the `/bin/bash` probe contract).
- **Connection: standard kube inference (in-cluster SA → KUBECONFIG → ~/.kube/config) with explicit `kubeconfig`/`context`/`namespace` overrides under `[server.sandbox.providers.kubernetes]`; optional `imagePullSecrets`** — covers both fabro-in-cluster and compose-pointing-at-remote-cluster; kubeconfig stays file-based, not vault-based.
- **Single namespace, server-level; per-environment cluster/namespace out of v1** — repo's documented posture is trusted-single-tenant; additive later if needed.
- **Provider registered only when cluster config resolves** (mirrors `enabled && daytona_api_key.is_some()`); policy default `enabled = true`.
- **Cargo feature `kubernetes`, default-off, enabled on fabro-server + fabro-cli** (mirrors daytona; release builds `-p fabro-cli` and feature-unifies through the server dependency).
- **Testing: unit tests for pod/label/policy generation, httpmock for REST CRUD, `#[ignore]`-gated live tests against a kind cluster for exec/clone/push** — matches `docker_streaming.rs` and the testing-strategy doc; no new twin artifact.
- **Docs (sandboxing.mdx, environments.mdx), changelog entry, diagnostics card, settings-sandboxes web card included** — convention-mandated; `cargo dev docs check` runs in CI.
- **Preview URLs and VNC out of v1** — would require an ingress/routing/auth design that doesn't exist; explicitly excluded (flagged in Q2 options so this is overridable).
- **Downgrade-after-upgrade caveat noted in changelog** — run records with `provider: "kubernetes"` won't deserialize on older binaries; same property Daytona's addition had.