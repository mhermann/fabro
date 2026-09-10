I have verified every structural fact I need. Here is the implementation plan.

# Implementation plan: Kubernetes sandbox provider

## Architecture (from the human's answers)

One pod per run sandbox. The pod's main container runs the **user's prebuilt image** with its command overridden to a **Fabro sandbox agent** — a static musl binary injected via an init container from a Fabro-published agent image (`ghcr.io/fabro-sh/fabro-sandbox-agent`). The agent serves HTTP+JSON on `127.0.0.1:7800` inside the pod (exec, streaming exec, long-running processes); the provider reaches it through a kube **port-forward** (works identically in-cluster and off-cluster, needs only `pods` + `pods/portforward` RBAC — no pod-exec RBAC). Cluster resolution is ambient (`kube` client `try_default`: in-cluster ServiceAccount or `~/.kube/config` via `HOME`, which survives the worker env allowlist). Config validation rejects `image.dockerfile` and network modes other than `allow_all`. No terminal, preview URLs, VNC, or install-wizard entry.

## New files

| Path | Contents |
|---|---|
| `lib/components/fabro-sandbox/src/kubernetes/mod.rs` | `KubernetesSandbox` (impl `Sandbox`: initialize → bash probe → clone via exec using the existing shared `setup_git_via_exec`/`git_retry`/push-credential machinery, exactly the Docker pattern), `KubernetesSandboxOptions` (image default `buildpack-deps:noble`, env vars, cpu/memory/disk, clone_depth, skip_clone, agent_image, agent_port), `reconnect(pod_name, …)`, `WORKING_DIRECTORY = "/workspace"`, `REPOS_ROOT = "/repos"`, `RUNTIME_DIRECTORY = "/tmp/fabro/runtime"` |
| `lib/components/fabro-sandbox/src/kubernetes/pod.rs` | Pure pod-manifest builder: pod name `fabro-run-{run_id}` (DNS-1123-safe), managed labels (`sh.fabro.managed`/`sh.fabro.run_id` + validated user labels), init container copying the agent binary (`fabro-sandbox-agent self-install /fabro/bin/…`) into a shared `emptyDir`, main container (user image, command overridden to the agent, `FABRO_SANDBOX_AGENT_TOKEN` + user env, resources requests=limits from cpu/memory, ephemeral-storage limit from disk, `emptyDir` mounts for `/workspace`, `/repos`, `/fabro`). Unit-testable without a cluster |
| `lib/components/fabro-sandbox/src/kubernetes/agent_client.rs` | HTTP client over the port-forward stream: `exec` (JSON-lines chunked: stdout/stderr b64 + exit), `process spawn/stdin/terminate/wait` mapping to `ExecStreamingResult`/`StdioProcess`. Bearer-token auth (per-pod random token; pod-internal trust, documented) |
| `lib/components/fabro-sandbox/src/provider/kubernetes.rs` | `KubernetesSandboxProvider` (list pods by label selector, get, delete pod with managed-label refusal, `create` for registry parity) |
| `lib/components/fabro-sandbox/src/details/kubernetes.rs` | Pod → `SandboxInfo` mapping (state: Pending→creating, Running→running, Succeeded/Failed→stopped; resources from pod spec) |
| `lib/apps/fabro-sandbox-agent/Cargo.toml`, `src/main.rs`, `src/protocol.rs` | Static-musl agent binary: axum/tokio HTTP server; `/exec`, `/processes` endpoints; PID-1 duties (zombie reaping via subreaper); `self-install` subcommand for the init container |
| `lib/apps/fabro-sandbox-agent/Dockerfile` | Multi-stage: cargo-zigbuild musl → `gcr.io/distroless/static-debian12` |
| `lib/foundation/fabro-dev/src/commands/agent_image.rs` | `cargo dev agent-image` — mirrors `docker_build.rs` (same zigbuild/musl pipeline) for the agent image |
| `docs/public/changelog/2026-09-10.mdx` | Changelog entry (date per convention) |

## Modified files, in order

**Step 1 — scaffolding**
- `Cargo.toml` (workspace): add `kube` and `k8s-openapi` workspace deps.
- `lib/components/fabro-sandbox/Cargo.toml`: `kubernetes = ["dep:kube", "dep:k8s-openapi"]` feature + deps.
- `lib/apps/fabro-server/Cargo.toml`, `lib/apps/fabro-cli/Cargo.toml`, `lib/components/fabro-workflow/Cargo.toml`: add `"kubernetes"` to `fabro-sandbox` features (workspace won't compile between steps 2–9; that is expected and resolved by compiler-driven completeness — every exhaustive `match` on the enums gets an arm).

**Step 2 — `lib/foundation/fabro-types/src/sandbox_provider.rs`, `settings/run.rs`, `settings/server.rs`**
- `SandboxProviderKind::Kubernetes` + `EnvironmentProvider::Kubernetes` variants (strum/serde → `"kubernetes"`), extend both `is_clone_based()`, `From<EnvironmentProvider>` arm; `ServerSandboxProvidersSettings` gains `kubernetes: ServerSandboxProviderSettings` + `for_provider` arm. Update the three enum unit-test files (`sandbox_provider.rs` tests, `tests/sandbox_model_serde.rs`, `tests/sandbox_inventory_serde.rs`).

**Step 3 — `lib/foundation/fabro-config/src/resolve/environment.rs`, `resolve/server.rs` (+ `tests/resolve_run.rs`)**
- `validate_provider_capabilities`: Kubernetes arm rejects `image.dockerfile` ("prebuilt image refs only…"), rejects `network.mode` `block`/`cidr_allow_list`, rejects invalid Kubernetes label keys/values (63-char value limit).
- `resolve/server.rs`: default the new `kubernetes` provider entry to enabled (presence-gated default, matching existing providers).

**Step 4 — mechanical enum-arm sites**
- `lib/components/fabro-environment/src/store.rs`: add `KUBERNETES_DEFAULT_ENVIRONMENT_TOML` (docker-image default) + seed-match arm.
- `lib/components/fabro-store/src/run_state.rs`: `sandbox_plan` shows `image` for Kubernetes too (it is image-based).

**Step 5 — `lib/components/fabro-sandbox`**
- `src/error.rs`: kube-error → `crate::Error` context helpers (mirror `docker_connect`).
- `src/managed_labels.rs`: extend `#[cfg]` gates to the `kubernetes` feature.
- `src/kubernetes/*` (new, above), `src/sandbox_spec.rs`: `SandboxSpec::Kubernetes {config, github_app, run_id, clone_origin_url, clone_branch, clone_tag, clone_commit_sha}` + `build()` (port-forward setup deferred to first use) + `to_run_sandbox_instance` mirroring Docker's layout metadata.
- `src/provider.rs`: `SandboxCreateSpec::Kubernetes` variant.
- `src/from_environment.rs`: `kubernetes_config_from_environment` (+ agent image resolved from `FABRO_KUBERNETES_AGENT_IMAGE` env by spec-build time).
- `src/reconnect.rs`: Kubernetes arm → `KubernetesSandbox::reconnect` (missing pod → clear "workspace was ephemeral" error).
- `src/terminal.rs`: Kubernetes arm → explicit "Kubernetes sandboxes do not support terminals" error (Local precedent).
- `src/lib.rs`: module + re-exports, all `#[cfg]` gates extended.
- `lib/foundation/fabro-static/src/env_vars.rs` (+ `secret_registry.rs` if convention requires): `FABRO_KUBERNETES_AGENT_IMAGE` constant in the non-secret known-vars list.

**Step 6 — agent binary + image tooling** (files above).

**Step 7 — `lib/components/fabro-workflow/src/operations/start.rs`**
- `SandboxSpec::Kubernetes` construction arm + `resolve_kubernetes_config` (secrets-aware, mirroring the Docker path); provider-specific tests mirroring the Docker/Daytona ones (dry-run coercion, none-target, clone defaults).

**Step 8 — `lib/apps/fabro-server`**
- `src/server.rs` `build_sandbox_provider_registry`: register when enabled **and** `kube::Client::try_default()` succeeds (Daytona-style gating; skip with `debug!` log otherwise).
- `src/run_manifest.rs`: `preflight_sandbox_spec` Kubernetes arm (`skip_clone = true`, agent image from vault/env), `clone_disabled_for_provider` arm, Daytona-only error message generalized.
- `src/server/handler/sandbox.rs`: ssh-access arm → CONFLICT "does not support access commands" (Local precedent); VNC/preview already generic `!= Daytona` — no change.
- `src/server/handler/runs.rs` (~1106–1119): include Kubernetes in the "no provider-side build" arm.

**Step 9 — API contract**
- `docs/public/api-reference/fabro-api.yaml`: `SandboxProviderKind` enum + `kubernetes`; both `EnvironmentProvider` enums; `ServerSandboxProvidersSettings` `required` + `kubernetes` property.
- `cargo build -p fabro-api` regenerates Rust types (SandboxProviderKind stays replaced with `fabro_types::` via existing `with_replacement`); extend `lib/foundation/fabro-api/tests/` round-trips if variant lists are asserted.
- `cd lib/packages/fabro-api-client && bun run generate`.

**Step 10 — web UI**
- `apps/fabro-web/app/lib/environment-providers.ts`: add `KUBERNETES` to `CREATABLE_PROVIDERS`.
- `apps/fabro-web/app/components/environment-form.tsx`: for Kubernetes hide/disable dockerfile source (image ref only, with hint) and restrict network mode to `allow_all`.
- `apps/fabro-web/app/routes/settings-sandboxes.tsx`: provider display if it enumerates kinds (verify at edit time).

**Step 11 — docs**
- `docs/public/execution/environments.mdx`: Kubernetes section (image-only, allow_all-only, bash+git+sleep image contract, agent sidecar + registry pullability, stop-deletes-pod/workspace-ephemerality, RBAC list, ambient kubeconfig).
- `docs/public/administration/sandboxing.mdx`: four providers; inventory + RBAC notes.
- Changelog entry (new file above).

## Verification

- Per-crate then full: `cargo nextest run -p fabro-types -p fabro-config -p fabro-environment -p fabro-store -p fabro-sandbox -p fabro-workflow -p fabro-server -p fabro-api`, then `cargo nextest run --workspace`.
- `cargo +nightly-2026-04-14 fmt --check --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- Insta: `cargo insta pending-snapshots` after tests; review before accepting (enum additions may touch settings/install snapshots).
- Web: `cd apps/fabro-web && bun test && bun run typecheck`; regenerated client diff reviewed.
- New unit tests (no cluster needed): config mapping, validation rules, pod manifest builder (labels/volumes/resources/env/wrap), pod→`SandboxInfo` mapping, spec→record metadata, workflow start-spec construction, agent protocol (spawn/stdin/terminate against local processes).
- **Manual, because no cluster CI exists**: `cargo dev agent-image` → `kind load docker-image` → create a Kubernetes environment via `/api/v1/environments` → run a workflow (clone, exec, file edit, git push) → verify preflight report, `/api/v1/sandboxes` list/get, terminal endpoint returns the explicit unsupported error, run deletion deletes the pod, `lifecycle.preserve` leaves it running. Live-cluster integration tests are added `#[ignore]`d with a reason, per `docs/internal/testing-strategy.md`.

## Deliberately not doing

- **No NetworkPolicy enforcement** — `block`/`cidr_allow_list` rejected at validation (human's answer; honest-rejection precedent).
- **No dockerfile builds** — prebuilt image refs only (human's answer); no registry settings, no snapshot identity.
- **No PTY terminal, preview URLs/VNC, install-wizard entry** — core parity only (human's answer); explicit "not supported" errors.
- **No cluster settings schema** — ambient discovery only (human's answer); the single new settings key `server.sandbox.providers.kubernetes.enabled` exists because the existing schema enumerates providers.
- **No PVCs** — workspaces are ephemeral; `stop_on_terminal` deletes the pod and run state persists via git checkpoints; documented.
- **No `auto_stop`** idle reclamation (Docker precedent ignores it), **no CRDs/operators**, **no multi-cluster**, **no kind-in-CI**, **no changes to Local/Docker/Daytona behavior**, **no worker env-allowlist changes** (ambient `~/.kube/config` reaches workers via `HOME`; `KUBECONFIG` env intentionally does not — documented).