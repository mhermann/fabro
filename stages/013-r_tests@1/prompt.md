Goal: Create a provider for running sandboxes in Kubernetes
Run ID: 01M268NZXKBMFJAX0JGENZBZTE
Pipeline progress: 10 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (10.5 KB; full value: `/tmp/fabro/runtime/blobs/9dc2443aa717007b10d660cdaad87d4e4a25075fb52cbf18723ccb7b26b985c3.json`)
  Preview: Exploration complete. Here is my analysis.
  
  ---
  
  ## 1. Repository structure and where this change lands
  
  **Language/tooling:** Rust workspace (edition 2021, ~50 crates in `lib/apps`, `lib/components`, `lib/foundation`), plus a TypeScript React SPA (`apps/fabro-web`, Bun-bundled), an OpenAPI-first AP…

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (16.7 KB; full value: `/tmp/fabro/runtime/blobs/9805ce4443ecb0975a12ed8e29b1e0baa7d7043d5a5f02180eb8758a2e55652b.json`)
  Preview: I've now read the remaining surfaces I needed: provider-capability validation, install-wizard handling, the `Sandbox` trait's required-vs-defaulted methods, worker resume/reconnect flow, Daytona's streaming-exec transport, diagnostics, the CLI/server dependency graph (which determines feature unific…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > Both answers are in. Restating for the planning stage:
  > 
  > **Q1 — Deliverable shape: "Fourth sandbox provider."**
  > A new `SandboxProviderKind::Kubernetes` whose backing infrastructure is a Kubernetes cluster: one Pod per run, exec'd into by the server via kubeconfig or in-cluster ServiceAccount, built on the existing Docker/Daytona provider pattern. This means: no Helm chart / fabro-on-k8s deployment packaging, and the change lands in the sandbox provider stack (`fabro-sandbox`, `fabro-types`, `fabro-config`, `fabro-server`, `fabro-workflow`, OpenAPI/TS enums, settings-sandboxes web page, docs).
  > 
  > **Q2 — v1 surface: "Core only."**
  > v1 = create Pod, clone, exec + streaming exec, file operations, git push, stop/delete, inventory listing, diagnostics, docs. Concretely excluded: embedded terminal (no `TerminalSession` impl — `open_terminal_for_run` returns "not supported" for Kubernetes), no install-wizard entry (`InstallSandboxInput` stays `docker | daytona`), no preview URLs/VNC, and `image.dockerfile` rejected for kubernetes environments with a provider-capability validation error.
  > 
  > Combined with the sweep's decisions, the build is now fully determined: pod-per-run primitive, kube-rs client, exec-based cloning inheriting the shared git machinery, NetworkPolicy enforcement for `block`/`cidr_allow_list`, `resources.disk` → ephemeral-storage limits, `stop()` deletes the pod, server-level single namespace with kube-standard connection inference, `kubernetes` cargo feature enabled on fabro-server + fabro-cli, registration gated on resolvable cluster config, and httpmock + live-gated kind tests. Nothing remaining that would send me back for a second round.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > The answers covered the forks that mattered — deployment shape, config surface, and the clone/security contract. What's left (client library choice, internal pod layout) is implementation detail, not a material fork. Proceeding.
  > 
  > What I now understand the human wants:
  > 
  > - **A new Kubernetes provider in `fabro-sandbox`** implementing the existing `Sandbox` trait, as a peer of Docker and Daytona — not a replacement. Docker stays the default in `defaults.toml`; Kubernetes is opt-in via provider selection.
  > - **Same clone-based workspace contract as the other remote providers:** GitHub origin → clone into `/workspace`; non-GitHub origins fail unless `skip_clone = true`; absent origin or `skip_clone = true` → empty workspace. Exact-commit support follows the established rules: non-empty branch plus SHA, shallow-fetch of the SHA, checkout, admitted branch pointed at the commit, HEAD verified, and never a fallback to a newer branch HEAD. The Kubernetes transport stays distinct — shared code limited to the trait and common abstractions, no reuse of Docker/Daytona transport internals.
  > - **Config through provider settings:** kubeconfig path (or in-cluster detection), context, namespace, image, and resource limits — no new top-level config concepts.
  > - **Trusted-caller security posture**, same as the Docker provider: the operator is responsible for cluster access and isolation; we don't add multi-tenant hardening in this iteration.
  > 
  > Decisions I'm making on the human's behalf (small gaps, not worth another round):
  > 
  > - Use the Rust `kube` crate in-process rather than shelling out to `kubectl`, so the provider behaves like the other in-process transports and streams exec output natively.
  > - One Pod per sandbox with the clone/checkout done in an init container; the agent runs commands via the exec API.
  > - Track sandboxes with labels for ownership/cleanup, mirroring how Docker identifies run containers.
  > - Tests: unit tests with fixture manifests, plus env-gated live e2e tests behind the existing `e2e` profile rather than requiring a cluster by default.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (18.1 KB; full value: `/tmp/fabro/runtime/blobs/843b4e45dae3bbfb1f2ff842b1d8caea7f2d7ba50bcf18a4a593e025e06b7cd8.json`)
  Preview: All critique points verified against the code — `StdioProcessTermination::exited(Option<i32>)` makes `exit_code: None` legal, `automation-form.tsx` filters environments through `isCloneBasedEnvironment` at two sites, and the handler's Docker access arm is a clean reconnect + `ssh_access_command()` s…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is my second pass over this plan. Round one found two blocking defects (missing web environments surfaces with the automations gating coupling; the `sandbox_info`/`runtime.id` contradiction) and four named corrections. I've re-attacked rev 2 against the code rather than my memory of it, hunting specifically for new blocking defects and for round-one concerns that were addressed only superficially.
  > 
  > ## Round-one points: verified as genuinely fixed
  > 
  > - **Web surfaces**: `CREATABLE_PROVIDERS` in `environment-providers.ts` is the single source both `settings-environments.tsx` (line 71 picker filter) and `automation-form.tsx` (lines 126/301) consume — I re-read both consumers; the one-file fix claim is correct, and `parseCreatableProvider`'s Docker-default (verified in the form source) is patched. The ordering is also right: client regen (Step 7) precedes the web step so `providers[provider].enabled` typechecks.
  > - **Identity**: bare pod name everywhere, namespace re-inferred — consistent across `to_run_sandbox_instance`, `reconnect`, `details`, and registry `get`. The reconnect round-trip is now in the live test list.
  > - **Pod-name lowercasing, `clone_source` gate, no-sentinel-for-stdio (`StdioProcessTermination::exited(None)` is legal — verified against the type), kube feature flags**: all folded in.
  > 
  > ## New attack — what I checked and what remains
  > 
  > I went looking for fresh blocking defects in the risky spots:
  > 
  > - **Sentinel vs. binary/streamed output**: the hold-back-the-final-line design survives the nasty cases (no-trailing-newline output, binary tar payloads where the marker must be stripped before bytes reach the downloaded file, marker-collision-in-output). Fiddly, but unit-testable as planned. Implementation-level, not plan-level.
  > - **Dependency compatibility**: kube 2.x's hyper/tower stack vs. the workspace's reqwest 0.13/rustls tree — any conflict is a build-time `cargo` failure with a clear error, resolved by adjusting the kube version/features. Annoying, not redo-work.
  > - **Exhaustive-match coverage**: cross-checked every non-test `SandboxProviderKind`/`EnvironmentProvider` match site from my earlier repo-wide grep against the plan's file list — `automations.rs`, `handler/sandboxes.rs`, `run_files.rs`, `fabro-api/build.rs` use conversions/comparisons/Display only and need no arms; everything with a real match is listed. The compiler forces the rest.
  > - **Registry/reconnect namespace consistency** under `Config::infer()`: same-process workers and server restarts re-infer identically. Sound.
  > - **`restartPolicy: Never` + node loss**: pod goes Failed and stays; resume errors cleanly. Documented behavior, not a defect.
  > - **Install-wizard TS types**: `InstallSandboxInput` stays `docker|daytona` so no install-flow breakage; any web-side exhaustive switch on the widened enums fails `bun run typecheck`, which is in the verification list.
  > 
  > **Residual non-blocking items** (ordinary judgment calls for the implementer, named so they're not discovered late): the exact `k8s-openapi` version feature must match the kube build or compilation fails loudly; the environment-form's per-provider network-mode UI needs a look when implementing the kubernetes branch (server validation is authoritative either way); the marker parser must anchor on the *last* line only. None of these would produce wrong shipped behavior if handled at implementation time, and all are caught by the planned unit/typecheck gates.
  > 
  > The CI-coverage gap for exec/streaming/tar transport is the same concern I raised in round one; it matches this repo's Docker convention exactly (`docker_streaming.rs` is `#[ignore]`d; CI's `--run-ignored only` covers only fabro-agent/fabro-llm/twin-openai), the plan discloses it, and it specifies a concrete manual kind smoke. Re-raising it would be re-litigating an accepted, convention-consistent tradeoff.
  > 
  > No new blocking defects; the plan's file enumeration is now consistent with the repository as it actually is, its internal design contradictions are resolved, and the remaining unknowns fail loudly at build/test time rather than shipping silently wrong.
  > 
  > {"preferred_next_label": "approve"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (18.1 KB; full value: `/tmp/fabro/runtime/blobs/843b4e45dae3bbfb1f2ff842b1d8caea7f2d7ba50bcf18a4a593e025e06b7cd8.json`)
  Preview: All critique points verified against the code — `StdioProcessTermination::exited(Option<i32>)` makes `exit_code: None` legal, `automation-form.tsx` filters environments through `isCloneBasedEnvironment` at two sites, and the handler's Docker access arm is a clean reconnect + `ssh_access_command()` s…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is my second pass over this plan. Round one found two blocking defects (missing web environments surfaces with the automations gating coupling; the `sandbox_info`/`runtime.id` contradiction) and four named corrections. I've re-attacked rev 2 against the code rather than my memory of it, hunting specifically for new blocking defects and for round-one concerns that were addressed only superficially.
  > 
  > ## Round-one points: verified as genuinely fixed
  > 
  > - **Web surfaces**: `CREATABLE_PROVIDERS` in `environment-providers.ts` is the single source both `settings-environments.tsx` (line 71 picker filter) and `automation-form.tsx` (lines 126/301) consume — I re-read both consumers; the one-file fix claim is correct, and `parseCreatableProvider`'s Docker-default (verified in the form source) is patched. The ordering is also right: client regen (Step 7) precedes the web step so `providers[provider].enabled` typechecks.
  > - **Identity**: bare pod name everywhere, namespace re-inferred — consistent across `to_run_sandbox_instance`, `reconnect`, `details`, and registry `get`. The reconnect round-trip is now in the live test list.
  > - **Pod-name lowercasing, `clone_source` gate, no-sentinel-for-stdio (`StdioProcessTermination::exited(None)` is legal — verified against the type), kube feature flags**: all folded in.
  > 
  > ## New attack — what I checked and what remains
  > 
  > I went looking for fresh blocking defects in the risky spots:
  > 
  > - **Sentinel vs. binary/streamed output**: the hold-back-the-final-line design survives the nasty cases (no-trailing-newline output, binary tar payloads where the marker must be stripped before bytes reach the downloaded file, marker-collision-in-output). Fiddly, but unit-testable as planned. Implementation-level, not plan-level.
  > - **Dependency compatibility**: kube 2.x's hyper/tower stack vs. the workspace's reqwest 0.13/rustls tree — any conflict is a build-time `cargo` failure with a clear error, resolved by adjusting the kube version/features. Annoying, not redo-work.
  > - **Exhaustive-match coverage**: cross-checked every non-test `SandboxProviderKind`/`EnvironmentProvider` match site from my earlier repo-wide grep against the plan's file list — `automations.rs`, `handler/sandboxes.rs`, `run_files.rs`, `fabro-api/build.rs` use conversions/comparisons/Display only and need no arms; everything with a real match is listed. The compiler forces the rest.
  > - **Registry/reconnect namespace consistency** under `Config::infer()`: same-process workers and server restarts re-infer identically. Sound.
  > - **`restartPolicy: Never` + node loss**: pod goes Failed and stays; resume errors cleanly. Documented behavior, not a defect.
  > - **Install-wizard TS types**: `InstallSandboxInput` stays `docker|daytona` so no install-flow breakage; any web-side exhaustive switch on the widened enums fails `bun run typecheck`, which is in the verification list.
  > 
  > **Residual non-blocking items** (ordinary judgment calls for the implementer, named so they're not discovered late): the exact `k8s-openapi` version feature must match the kube build or compilation fails loudly; the environment-form's per-provider network-mode UI needs a look when implementing the kubernetes branch (server validation is authoritative either way); the marker parser must anchor on the *last* line only. None of these would produce wrong shipped behavior if handled at implementation time, and all are caught by the planned unit/typecheck gates.
  > 
  > The CI-coverage gap for exec/streaming/tar transport is the same concern I raised in round one; it matches this repo's Docker convention exactly (`docker_streaming.rs` is `#[ignore]`d; CI's `--run-ignored only` covers only fabro-agent/fabro-llm/twin-openai), the plan discloses it, and it specifies a concrete manual kind smoke. Re-raising it would be re-litigating an accepted, convention-consistent tradeoff.
  > 
  > No new blocking defects; the plan's file enumeration is now consistent with the repository as it actually is, its internal design contradictions are resolved, and the remaining unknowns fail loudly at build/test time rather than shipping silently wrong.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I mapped `docker.rs`'s actual structure (the 3,101-line module the plan builds beside) and re-examined the shared surface in `sandbox.rs`, looking specifically for the plan working around the code, missing seams, and duplication about to multiply.
  > 
  > ## What I looked at and found
  > 
  > **The genuinely shared layer already exists and is load-bearing.** `sandbox.rs` already holds the provider-neutral logic as free functions over `&dyn Sandbox` — `setup_git_via_exec`, `git_push_via_exec`, `fetch_source_run_ref`, `build_remote_walk_command`/`parse_remote_walk_output`, `resolve_path`/`join_sandbox_path`, the bash-probe contract — all cfg-gated `any(docker, daytona)` and all consumed by both existing providers. The plan's kubernetes module reuses these directly. That is the seam, and it's the right one.
  > 
  > **The one real duplication candidate — and why it doesn't trigger a pre-emptive refactor.** Docker's clone-with-retry block (`clone_github_repo`, `retry_git_transfer`, `run_exact_local_git_command`, `git_clone_command`, the failure classifiers — roughly lines 787–1125 and 1588–1615, ~400 lines) is provider-neutral logic that a naive kubernetes implementation would copy. But the repo's own history shows exactly how this is meant to evolve: `git_push_via_exec` and `setup_git_via_exec` were extracted into `sandbox.rs` over `&dyn Sandbox` *when the second consumer arrived*, and the plan's Step 5 plus its existing edits to `sandbox.rs` accommodate doing the same for the clone block **during implementation, guided by the real second consumer**. Extracting it *now* — before kubernetes exists — means guessing the seam: does the kubernetes side need Docker's stop-file/pid-file exec-control mechanism (plan says no — kill-by-pgid), the `docker_exec_shell` variants, or just trait-level `exec_command`? A pre-emptive extraction risks carving the wrong boundary and getting reworked. This is the documented-in-AGENTS.md, contract-heavy clone code; extracting it behavior-preserving-first with only daemon-gated tests is its own risk. The implementation-time extraction keeps the kubernetes diff honest and is reviewable as "move, don't modify."
  > 
  > **Everything else the plan touches is the repo's established extension pattern, not a workaround:**
  > - The ~10 cfg-gate widenings (`any(docker, daytona)` → `+kubernetes`) are mechanical; Cargo has no feature aliases, so no cheaper seam exists.
  > - Enum arms across `sandbox_spec.rs`, `provider.rs`, `reconnect.rs`, `details.rs`, `terminal.rs`, `start.rs`, `run_manifest.rs` are exactly how Daytona was added — extensibility by variant, compiler-enumerated.
  > - File ops (read/write/grep) are thin per-provider shells over each transport and are *already* duplicated between Docker and Daytona; a third thin implementation is the architecture's price, with the nontrivial command builders already shared.
  > - Exec transports are inherently provider-specific (bollard exec API / Daytona session / kube WebSocket) — a unifying "transport trait" would be speculative abstraction, not a needed seam.
  > - Testability holds in the current shape: the plan's unit tests target pure functions (wrapper builder, sentinel parser, policy construction) inside the new module; transport-dependent behavior is live-gated per repo convention, same as Docker today.
  > - No unit the plan grows is already too long (docker.rs/daytona aren't grown; sandbox.rs gains only cfg widenings and possibly the clone extraction, which would *shrink* docker.rs), and no name stops matching (`clone_source`, `REMOTE_BASH`, `from_environment` all stay honest).
  > 
  > ## Named but declined
  > 
  > - **A unified exec-transport abstraction across Docker/Daytona/kubernetes** — worth its own goal someday (Daytona's 5,093-line everything-itself module is pre-existing debt), but unrelated to making this change land, and too large to fold in.
  > - **The clone-block extraction as a standalone pre-refactor** — deliberately declined for the seam-guessing reason above; it belongs inside this change's Step 5, mirroring the `git_push_via_exec` precedent.
  > 
  > No refactoring is needed first: the implementation is not substantially harder without one, the duplication it would introduce is bounded and has an established in-repo resolution pattern, and the change is testable in the current shape.
  > 
  > {"preferred_next_label": "none"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/Cargo.toml, /workspace/fabro/apps/fabro-web/app/components/environment-form.test.ts, /workspace/fabro/apps/fabro-web/app/components/environment-form.tsx, /workspace/fabro/apps/fabro-web/app/components/environment-providers.test.ts, /workspace/fabro/apps/fabro-web/app/lib/environment-providers.test.ts, /workspace/fabro/apps/fabro-web/app/lib/environment-providers.ts, /workspace/fabro/apps/fabro-web/app/routes/settings-sandboxes.tsx, /workspace/fabro/docs/public/administration/sandboxing.mdx, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/changelog/2026-09-10.mdx, /workspace/fabro/docs/public/execution/environments.mdx, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/runs.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/sandbox.rs, /workspace/fabro/lib/apps/fabro-server/src/server/tests.rs, /workspace/fabro/lib/components/fabro-environment/src/store.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/details.rs, /workspace/fabro/lib/components/fabro-sandbox/src/error.rs, /workspace/fabro/lib/components/fabro-sandbox/src/from_environment.rs, /workspace/fabro/lib/components/fabro-sandbox/src/kubernetes.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/managed_labels.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider/kubernetes.rs, /workspace/fabro/lib/components/fabro-sandbox/src/reconnect.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/src/terminal.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/kubernetes_live.rs, /workspace/fabro/lib/components/fabro-workflow/Cargo.toml, /workspace/fabro/lib/components/fabro-workflow/src/operations/start.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/environment.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_run.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-types/src/sandbox_provider.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/run.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/tests/sandbox_model_serde.rs
- Response:
  > The Kubernetes sandbox provider is implemented and verified. Here's the full summary:
  > 
  > ## What was built
  > 
  > A fourth sandbox provider (`kubernetes`, clone-based like Docker/Daytona): one Pod per run, GitHub clone into `/workspace` via the shared exec machinery, agent commands run through the Kubernetes exec API, connection via kube-standard inference (in-cluster SA → `KUBECONFIG` → `~/.kube/config`), `stop()` deletes the pod, per-pod NetworkPolicy enforces `block`/`cidr_allow_list` (owner-referenced for GC).
  > 
  > ## File-by-file changes
  > 
  > **New files**
  > - `lib/components/fabro-sandbox/src/kubernetes.rs` — the provider core (~3,000 lines): `KubernetesSandboxOptions`/`KubernetesNetworkMode`, pod manifest (labels, quantities with requests=limits, `restartPolicy: Never`), NetworkPolicy manifest (block/CIDR+DNS), pod-ready polling with image-pull/crash-loop terminal detection, exec transport (per-exec control files + `setsid` process-group kill + per-exec-nonce `__FABRO_RC_*__=` stdout sentinel parsed/stripped by `ExecSentinelReader`), full `Sandbox` impl (initialize/probe/exec/streaming/stdio via stream-close → `exited(None)`/tar-over-exec file transfer/grep/walk/git lifecycle via shared helpers), 17 unit tests, plus 5 httpmock REST tests in `provider/kubernetes.rs` (label-selector list, 404→`None`, unmanaged refusal, delete idempotence).
  > - `lib/components/fabro-sandbox/src/provider/kubernetes.rs` — `SandboxProvider` impl with client-injectable `list/get/delete_with_client` for testability.
  > - `lib/components/fabro-sandbox/tests/kubernetes_live.rs` — `#[ignore]`-gated live tests (init/probe/stop, streaming exit codes, tar round-trip + reconnect, timeout kill), kind recipe in module docs.
  > - `docs/public/changelog/2026-09-10.mdx`, `apps/fabro-web/app/lib/environment-providers.test.ts`.
  > 
  > **Modified** — enum/cfg arms: `fabro-types` (`SandboxProviderKind`, `EnvironmentProvider`, `ServerSandboxProvidersSettings` + tests), `fabro-config` (server layer/resolve, kubernetes dockerfile rejection + tests), `fabro-sandbox` (lib.rs, sandbox.rs, sandbox_spec.rs, provider.rs, details.rs with phase→state mapping + quantity parsing tests, reconnect.rs, terminal.rs "no embedded terminals", from_environment.rs config mapping, error.rs `KubernetesConnect`, managed_labels.rs/clone_source.rs gate widenings), `fabro-environment` (seeded default), `fabro-install` (policy write, wizard unchanged per plan), workflow `start.rs`, server (`server.rs` registry, `run_manifest.rs`, `handler/runs.rs`, `handler/sandbox.rs`, diagnostics.rs new Kubernetes card, tests.rs policy test), OpenAPI spec + regenerated TS client, docs (`sandboxing.mdx`, `environments.mdx`), deps (`kube` 4.2/k8s-openapi 0.28 `latest`=v1_36, ring provider matching workspace rustls).
  > 
  > ## Adaptations from the plan
  > - **`clone_source` gates**: the plan listed one missed gate; the compile revealed several more (`exact_repository_init_command`, `pinned_fetch_command`, etc.) — all widened identically.
  > - **`finish()` sentinel anchoring**: devils flagged marker-collision and no-trailing-newline; implemented with a per-exec nonce + last-occurrence match so binary tar payloads strip cleanly (unit-tested).
  > - **Install policy**: the plan kept the wizard docker/daytona-only; I additionally write `kubernetes.enabled = false` during install so the written config expresses intent rather than relying on the default.
  > - **k8s-openapi 0.28 API drift**: `labels`/`match_labels` are `BTreeMap`, `creation_timestamp` is a jiff `Time` newtype, `pod_selector`/`limits` are `Option`, `NetworkPolicy` has no `status` — all adapted.
  > - **Provider testability**: provider methods delegate to `*_with_client` so httpmock tests don't need cluster inference.
  > 
  > ## Verification
  > fmt (pinned nightly) clean; `clippy --workspace --all-targets -D warnings` clean; tests: types 771 ✓, config ✓, api 222 ✓, sandbox 187 ✓ (46 live-skipped by design), server 904 ✓ (incl. API conformance), sandbox+workflow 1774 ✓, workspace 7454/7458 ✓; web tests+typecheck ✓; TS client typecheck ✓; `cargo dev docs check` ✓. The 4 workspace failures and the CLI `it` server-spawning tests fail **only** on this environment's exhausted shared disk (`InsufficientDisk: available_bytes: 0` preflight) — they passed in dedicated runs with disk headroom, and web test failures are pre-existing (identical on clean tree). Environment notes: I installed Rust (stable + pinned nightly), nextest, bun, and a JRE (for TS client generation); file mtimes in this sandbox default to epoch-0, which required `touch` passes before rebuilds.


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

Review the change for test coverage only.

Ask: does this change have tests, do they test behaviour rather than
implementation detail, and would they actually fail if the code were wrong?

Look for: new behaviour with no test, tests asserting what the code does rather
than what it should do, tests that pass regardless of the implementation,
missing failure and boundary cases, and tests coupled to internals in a way
that will break on any refactor.

Name the specific untested case. "Add more tests" is not a finding.

If coverage is adequate, say so plainly.
