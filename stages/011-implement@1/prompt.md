Goal: Create a provider for running sandboxes in Kubernetes
Run ID: 01M25MEFENW9C2NFF7G196FE8M
Pipeline progress: 9 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (12.7 KB; full value: `/tmp/fabro/runtime/blobs/b4986dfc9c21a72f408be3b7b25a60c44b4648e8fe0cbca868e38c61081e859a.json`)
  Preview: Exploration complete. I read the sandbox crate end-to-end, the provider trait/registry, both existing remote providers (Docker, Daytona), the config/settings layer, the server wiring, the OpenAPI surface, and the web UI. No files were modified. Here is the report.
  
  ## 1. Repository structure, langua…

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (16.5 KB; full value: `/tmp/fabro/runtime/blobs/c31c47cfac8caf510c2d078007b4889a66a5ce2500b2ffa80fc5fcd183e60628.json`)
  Preview: Deep pass complete. Below is the deliberate sweep, the bucket sort, the stress-test, and the final question set.
  
  # Dimension-by-dimension sweep
  
  **1. Scope boundaries.**
  Clearly in: a fourth provider kind wired through the whole stack (enums, `Sandbox` + `SandboxProvider` impls, config mapping, pol…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > All five answers are back. Restating them for the record — this is what the planning stage will build from:
  > 
  > ## Answers and what they mean
  > 
  > 1. **Exec transport → In-pod agent sidecar.** Each sandbox pod runs the user's image plus a Fabro agent sidecar; the provider talks to the agent (Daytona-toolbox-style) for exec streaming and stdio processes. Consequences: Fabro must publish an agent container image as a new release artifact (the cluster must be able to pull it); the server/worker needs Kubernetes RBAC only for pod lifecycle (create/get/list/delete — **no** pod-exec RBAC); streaming/PTY complexity moves into the agent protocol.
  > 
  > 2. **Image story → Prebuilt image refs only.** `image.docker` is honored as a pod image reference; `image.dockerfile` is rejected at config validation for Kubernetes environments with a clear remediation error (build and push elsewhere). No registry settings, no build machinery, no snapshot identity.
  > 
  > 3. **Capability scope → Core parity.** Full `Sandbox` trait (exec streaming, files, walk/grep/glob, git setup, push credentials), provider inventory list/get/delete, reconnect, config/policy/UI/API wiring, docs. **No** PTY web terminal, **no** preview URLs/VNC, **no** install-wizard entry — those return explicit "not supported" errors, exactly as Local does for terminals today.
  > 
  > 4. **Cluster targeting → Ambient only.** The kube client resolves the cluster from the in-cluster ServiceAccount or the process's default kubeconfig; zero new settings.toml schema; a single implicit cluster per server. Workers do **not** inherit ambient env (the fail-closed allowlist stays) — pod lifecycle credentials travel via the vault/FABRO_HOME pattern only where needed.
  > 
  > 5. **Network modes → Reject at validation.** Kubernetes environments accept only `network.mode = "allow_all"`; `block` and `cidr_allow_list"` are rejected in `fabro-config` validation with the Docker-precedent error style. No NetworkPolicy objects are created.
  > 
  > Combined shape: a clone-based, pod-per-run provider with a published agent sidecar image, prebuilt-image-only environments, ambient single-cluster discovery, core capability surface, and allow-all-only networking — everything else (naming `kubernetes`, `/workspace`+`/repos` layout, bash+git image contract, resource mapping requests=limits + ephemeral-storage, stop=delete-pod with git-checkpoint-based resume, managed-label inventory safety, vault credential transport, `#[ignore]`/env-gated live-cluster tests, additive OpenAPI/UI updates, docs + changelog) was already settled in the previous stage and stands.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > The answers I have are enough to build from. The remaining open points (kube-rs vs. shelling out to `kubectl`, Pod-per-run vs. Job vs. namespace-per-run) are implementation choices I can settle myself from repo conventions rather than product ambiguity — two engineers given these answers would converge on materially the same provider.
  > 
  > What I understand the human wants:
  > 
  > - A **Kubernetes sandbox provider** in `fabro-sandbox`, joining `local`, `docker`, and `daytona` behind the same `Sandbox` trait, so runs can execute in containers scheduled by a Kubernetes cluster (primarily for server deployments) instead of the operator's Docker daemon.
  > - It should follow the established **clone-based provider contract**: GitHub-only origins cloned into `/workspace`, optional exact commit always paired with a non-empty branch (shallow-fetch the SHA, pin the branch at it, verify HEAD, never fall back to branch HEAD), empty workspace when the origin is absent or `skip_clone = true`, and present non-GitHub origins fail setup.
  > 
  > Decisions I'm making on their behalf:
  > 
  > - **Client**: use `kube` (kube-rs) with `Config::infer` — in-cluster service account when running inside a pod, local kubeconfig otherwise. Cluster access, RBAC, and namespace provisioning are operator responsibilities, mirroring the existing stance on Docker daemon access.
  > - **Shape**: one Pod per sandbox (namespace, image, resource requests/limits, and optional node/affinity knobs from provider config) — not a Job controller or namespace-per-run orchestration in this iteration. Keep the clone transport distinct from Docker's, per the repo guidance that provider transports stay separate.
  > - **Config**: new `kubernetes` provider selectable via `defaults.toml`; Docker remains the default so this is opt-in.
  > - **Tests**: unit tests with a fake kube client; live-cluster tests gated behind the `e2e` profile with credentials from `.env`, honoring the `test_support` feature boundaries so no test shims leak into production builds.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (13.8 KB; full value: `/tmp/fabro/runtime/blobs/e9c359880410bf41fe6a4702e7d4fd5f643bd57e964c66c19821def04f793f40.json`)
  Preview: No refactoring stage ran; the codebase is unchanged since exploration. I re-verified the release-pipeline facts I'm newly naming (`fabro-dev/src/commands/`, `.github/workflows/release.yml` docker job pushing `ghcr.io/fabro-sh/fabro`). This revision integrates all three blocking criticisms; none reje…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is the second critique round. The three defects I raised before are each genuinely fixed — I re-verified the fixes against the repository rather than taking the plan's word:
  > 
  > - **Allowlist fix**: `KUBERNETES_SERVICE_HOST`/`PORT` are the actual env gates for kube-rs in-cluster discovery; adding them to `WORKER_ENV_ALLOWLIST` follows the existing AWS-chain precedent in that exact file, and the plan updates the fail-closed test that would otherwise fail. Correct.
  > - **Token fix**: HMAC over the vault `SESSION_SECRET` + pod name is re-derivable from every process that matters (worker via FABRO_HOME vault access — the mechanism `spawn_env.rs` documents; server via `vault_secret(EnvVars::SESSION_SECRET)` — the call pattern `worker_token_keys_from_server_secrets` already uses). Only the derived token, never the secret, enters pod env. The reconnect flow now has a sound basis.
  > - **Release fix**: `release.yml`/`nightly.yml` have a docker job pushing `ghcr.io/fabro-sh/fabro` that the agent job can mirror; the default image ref will resolve on release.
  > 
  > New attack surface I probed this round:
  > 
  > 1. **`FABRO_KUBERNETES_AGENT_IMAGE` override resolution** — the worker builds specs under `env_clear()`, so an ambient-env-only override would silently not apply to server runs. Non-blocking: the compiled-in default ref works regardless, and the fix is a resolution-order rule (secret store first, then process env — the same pattern the Daytona key uses) the implementer can apply without redesign. Flagging it here so it isn't discovered as a surprise.
  > 2. **Idle SPDY/port-forward longevity** for hour-long stdio processes — operational tuning (keepalive ping), not an architectural flaw; the manual kind checklist plus `#[ignore]`d integration tests cover the real behavior.
  > 3. **`cleanup()` vs `preserve` semantics** — not spelled out per-method, but "exactly the Docker pattern" plus the stated stop/delete/preserve decisions pins it; ordinary judgment call.
  > 4. **UI terminal tab on Kubernetes runs** will surface the explicit unsupported error — the agreed scope, not a regression.
  > 5. Re-checked the exhaustive-match census (run_state, environment store, install, demo, git_retry, run_progress are tests/fixtures/no-ops) — compiler-driven completeness plus the enumerated arms covers it; no hidden coupling found beyond what the plan names.
  > 6. Verification soundness: the restart-mid-run manual check is the test that would catch a wrong token scheme; the allowlist unit test catches a wrong env story; agent protocol tests are process-level, not mock-heavy. Nothing passes vacuously.
  > 
  > The remaining findings are implementer judgment calls that later review stages can catch cheaply. The plan is a sound basis for work.
  > 
  > {"preferred_next_label": "approve"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (13.8 KB; full value: `/tmp/fabro/runtime/blobs/e9c359880410bf41fe6a4702e7d4fd5f643bd57e964c66c19821def04f793f40.json`)
  Preview: No refactoring stage ran; the codebase is unchanged since exploration. I re-verified the release-pipeline facts I'm newly naming (`fabro-dev/src/commands/`, `.github/workflows/release.yml` docker job pushing `ghcr.io/fabro-sh/fabro`). This revision integrates all three blocking criticisms; none reje…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is the second critique round. The three defects I raised before are each genuinely fixed — I re-verified the fixes against the repository rather than taking the plan's word:
  > 
  > - **Allowlist fix**: `KUBERNETES_SERVICE_HOST`/`PORT` are the actual env gates for kube-rs in-cluster discovery; adding them to `WORKER_ENV_ALLOWLIST` follows the existing AWS-chain precedent in that exact file, and the plan updates the fail-closed test that would otherwise fail. Correct.
  > - **Token fix**: HMAC over the vault `SESSION_SECRET` + pod name is re-derivable from every process that matters (worker via FABRO_HOME vault access — the mechanism `spawn_env.rs` documents; server via `vault_secret(EnvVars::SESSION_SECRET)` — the call pattern `worker_token_keys_from_server_secrets` already uses). Only the derived token, never the secret, enters pod env. The reconnect flow now has a sound basis.
  > - **Release fix**: `release.yml`/`nightly.yml` have a docker job pushing `ghcr.io/fabro-sh/fabro` that the agent job can mirror; the default image ref will resolve on release.
  > 
  > New attack surface I probed this round:
  > 
  > 1. **`FABRO_KUBERNETES_AGENT_IMAGE` override resolution** — the worker builds specs under `env_clear()`, so an ambient-env-only override would silently not apply to server runs. Non-blocking: the compiled-in default ref works regardless, and the fix is a resolution-order rule (secret store first, then process env — the same pattern the Daytona key uses) the implementer can apply without redesign. Flagging it here so it isn't discovered as a surprise.
  > 2. **Idle SPDY/port-forward longevity** for hour-long stdio processes — operational tuning (keepalive ping), not an architectural flaw; the manual kind checklist plus `#[ignore]`d integration tests cover the real behavior.
  > 3. **`cleanup()` vs `preserve` semantics** — not spelled out per-method, but "exactly the Docker pattern" plus the stated stop/delete/preserve decisions pins it; ordinary judgment call.
  > 4. **UI terminal tab on Kubernetes runs** will surface the explicit unsupported error — the agreed scope, not a regression.
  > 5. Re-checked the exhaustive-match census (run_state, environment store, install, demo, git_retry, run_progress are tests/fixtures/no-ops) — compiler-driven completeness plus the enumerated arms covers it; no hidden coupling found beyond what the plan names.
  > 6. Verification soundness: the restart-mid-run manual check is the test that would catch a wrong token scheme; the allowlist unit test catches a wrong env story; agent protocol tests are process-level, not mock-heavy. Nothing passes vacuously.
  > 
  > The remaining findings are implementer judgment calls that later review stages can catch cheaply. The plan is a sound basis for work.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I looked for the specific failure modes — workarounds, missing seams, triplication, overgrown units — by reading the code the plan will sit beside, not the plan.
  > 
  > **What I found:**
  > 
  > 1. **The triplication risk is mostly already solved.** Docker and Daytona's file operations are *not* copies of each other: Docker rides its native tar/exec APIs (`download_file_bytes`, `upload_bytes_to_container`, `docker_exec`), Daytona rides its SDK's fs service (`fs_svc.download_file`, `create_folder`). There is no exec-based file-ops module existing twice that Kubernetes would copy a third time. The genuinely shared remote-sandbox logic — bash probe, walk-command building/parsing, path resolution, `setup_git_via_exec`, git retry, push credentials — was already extracted into `sandbox.rs`/`clone_source.rs`/`git_retry.rs` as `pub(crate)` helpers, cfg-gated `docker | daytona`. The repo has demonstrably maintained this seam; extending the gates with `kubernetes` is the designed extension point, not a workaround.
  > 
  > 2. **The remaining duplication is small-scale.** A K8s exec-based `list_directory`/`file_exists` will resemble Docker's (~15 lines each, different transport). Extracting an exec-file-ops mixin would serve 2 of 3 providers and save ~100–150 lines — below the "substantially harder/larger/riskier" bar; the implementer can extract in-change if it turns out clean.
  > 
  > 3. **The parallel `SandboxProviderKind`/`EnvironmentProvider` enums are real debt** — every provider addition touches both plus a `From` impl plus two `is_clone_based`s. But this change's cost for that debt is ~10 mechanical lines, and unifying the enums is a workspace-wide change with OpenAPI type-ownership implications — its own piece of work with its own conversation. Named for the record; not this change.
  > 
  > 4. **`SandboxSpec` variants repeat clone fields** (config/github_app/run_id/origin/branch/tag/sha per variant); a shared `CloneContext` would help, but restructuring the two existing variants churns every construction site in three crates for no reduction in risk to this change. Not needed.
  > 
  > 5. **No overgrown unit gets grown further** — the K8s provider is new files; `docker.rs`/`daytona/mod.rs` are untouched. The plan already embeds the two correctly-sized in-change refactors it needs (`ReconnectCredentials` param consolidation, BASH_ENV scrub helper move to `sandbox.rs`), which is the right size for them.
  > 
  > 6. **Testability is fine** — new modules are pure builders/mappings; the agent protocol is testable at process level; no seam is missing that verification depends on.
  > 
  > The change slots into seams the repository already has. Refactoring first would delay the work without making it meaningfully smaller or safer.
  > 
  > {"preferred_next_label": "none"}


Implement the approved plan.

The goal is:

Create a provider for running sandboxes in Kubernetes

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