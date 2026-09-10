No refactoring stage ran; the codebase is unchanged since exploration. I re-verified the release-pipeline facts I'm newly naming (`fabro-dev/src/commands/`, `.github/workflows/release.yml` docker job pushing `ghcr.io/fabro-sh/fabro`). This revision integrates all three blocking criticisms; none rejected. The non-blocking findings are folded in as named line items, not left implicit.

# Implementation plan (revised): Kubernetes sandbox provider

**Changed from the previous plan — all three critique defects accepted:**
1. **Worker env allowlist** (was: "no allowlist changes"): in-cluster kube discovery is env-gated (`KUBERNETES_SERVICE_HOST`/`PORT`), and sandbox construction runs in spawned workers under `env_clear()`. The allowlist now gains both vars, mirroring the existing AWS-chain precedent, and the fail-closed test is updated.
2. **Agent auth token** (was: random per-pod token): reconnect-from-another-process is load-bearing (`reconnect.rs` serves server-side file/exec endpoints and resume). The token is now deterministically derived — `HMAC-SHA256(vault SESSION_SECRET, "fabro-k8s-agent:" + pod_name)` — the direct analogue of `worker_token_keys_from_server_secrets` in `server.rs`, so any process that can read the vault re-derives it. Random state is eliminated.
3. **Agent image release wiring** (was: local dev command only): the release/nightly GitHub workflows gain an agent-image build+push job alongside the existing `ghcr.io/fabro-sh/fabro` docker job, so the default image ref actually resolves for users.

Also folded in from the critique's non-blocking list: explicit `#[cfg]`-gate extensions for `clone_source.rs`/`from_environment.rs`/`managed_labels.rs`; the BASH_ENV scrub helper moves to shared `sandbox.rs`; run-details tab defers via the existing `sandbox_details()` catch-all; `k8s-openapi` version feature is pinned explicitly.

## Architecture

One pod per run sandbox. Main container runs the **user's prebuilt image** with its command overridden to the **Fabro sandbox agent** — a static-musl binary injected by an init container from `ghcr.io/fabro-sh/fabro-sandbox-agent:<version>`. The agent serves HTTP+JSON on `127.0.0.1:7800` in the pod; the provider connects via kube **port-forward** (identical in-cluster and off-cluster; RBAC: pods get/list/create/delete + pods/portforward; no pod-exec). Agent auth: bearer token derived as HMAC over the vault `SESSION_SECRET`. Cluster resolution: `kube::Client::try_default()` (in-cluster SA or `~/.kube/config`). Validation rejects `image.dockerfile`, non-`allow_all` network modes, and invalid K8s label values. No terminal/preview/VNC/install-wizard.

## New files

| Path | Contents |
|---|---|
| `lib/components/fabro-sandbox/src/kubernetes/mod.rs` | `KubernetesSandbox` (impl `Sandbox`; initialize → bash probe → clone via exec, using shared `setup_git_via_exec`/`git_retry`/push-credential machinery — the Docker pattern); `KubernetesSandboxOptions` (image default `buildpack-deps:noble`, env vars, cpu/memory/disk, clone_depth, skip_clone, agent_image, agent_port, agent_token_key); `reconnect(pod_name, …)` re-deriving the token; `derive_agent_token(key, pod_name)` (HMAC-SHA256, hex-truncated; unit-tested for stability across calls); constants `WORKING_DIRECTORY="/workspace"`, `REPOS_ROOT="/repos"`, `RUNTIME_DIRECTORY="/tmp/fabro/runtime"` |
| `lib/components/fabro-sandbox/src/kubernetes/pod.rs` | Pure pod-manifest builder: name `fabro-run-{run_id}`; managed labels + validated user labels; init container (`fabro-sandbox-agent self-install /fabro/bin/fabro-sandbox-agent` into shared `emptyDir`); main container (user image, agent command override incl. derived token env, user env, resources requests=limits from cpu/memory, ephemeral-storage limit from disk, `emptyDir` mounts `/workspace`, `/repos`, `/fabro`). Unit-testable without a cluster |
| `lib/components/fabro-sandbox/src/kubernetes/agent_client.rs` | HTTP/1.1 client over the port-forward stream: `/exec` (JSON-lines chunked), `/processes` spawn/stdin/terminate/wait → `ExecStreamingResult`/`StdioProcess`; process-group kill on stream close |
| `lib/components/fabro-sandbox/src/provider/kubernetes.rs` | `KubernetesSandboxProvider` — list pods by managed-label selector, get, delete (refusing unmanaged pods), `create` for registry parity |
| `lib/components/fabro-sandbox/src/details/kubernetes.rs` | Pod → `SandboxInfo` (Pending→creating, Running→running, Succeeded/Failed→stopped; resources from pod spec). Inventory only — the run-details tab uses the existing `sandbox_details()` catch-all error |
| `lib/apps/fabro-sandbox-agent/Cargo.toml`, `src/main.rs`, `src/protocol.rs` | Static-musl agent: axum/tokio; `/exec`, `/processes`; bearer-token check; PID-1 duties (subreaper/zombie reaping); `self-install` copy subcommand |
| `lib/apps/fabro-sandbox-agent/Dockerfile` | Multi-stage cargo-zigbuild musl → `gcr.io/distroless/static-debian12` |
| `lib/foundation/fabro-dev/src/commands/agent_image.rs` | `cargo dev agent-image` mirroring `docker_build.rs` (same zigbuild pipeline; `--compile-only`/`--dry-run`) + registration in `commands/mod.rs`/`lib.rs` |
| `docs/public/changelog/2026-09-10.mdx` | Changelog entry |

## Modified files, in order

**Step 1 — scaffolding**
- `Cargo.toml` (workspace): `kube` + `k8s-openapi` (one version feature pinned, e.g. latest supported by the pinned `kube`).
- `lib/components/fabro-sandbox/Cargo.toml`: `kubernetes = ["dep:kube", "dep:k8s-openapi"]` + deps.
- `lib/apps/fabro-server/Cargo.toml`, `lib/apps/fabro-cli/Cargo.toml`, `lib/components/fabro-workflow/Cargo.toml`: add `"kubernetes"` to `fabro-sandbox` features. (Workspace intentionally non-compiling until step 8; exhaustive-match errors drive completeness.)

**Step 2 — `lib/foundation/fabro-types/src/sandbox_provider.rs`, `settings/run.rs`, `settings/server.rs`**
- `SandboxProviderKind::Kubernetes` + `EnvironmentProvider::Kubernetes` (strum/serde → `"kubernetes"`); both `is_clone_based()` extended; `From` impl arm; `ServerSandboxProvidersSettings.kubernetes` + `for_provider` arm. Unit tests extended in `sandbox_provider.rs`, `tests/sandbox_model_serde.rs`, `tests/sandbox_inventory_serde.rs`.

**Step 3 — `lib/foundation/fabro-config/src/resolve/environment.rs`, `resolve/server.rs` (+ `tests/resolve_run.rs`)**
- Kubernetes validation arm: reject `image.dockerfile`, reject `network.mode` `block`/`cidr_allow_list`, reject invalid K8s label keys/values (63-char limit).
- `resolve/server.rs`: presence-gated default `kubernetes.enabled = true`, matching existing providers.

**Step 4 — mechanical enum arms**
- `lib/components/fabro-environment/src/store.rs`: `KUBERNETES_DEFAULT_ENVIRONMENT_TOML` + seed-match arm.
- `lib/components/fabro-store/src/run_state.rs`: `sandbox_plan` exposes `image` for Kubernetes (image-based like Docker).

**Step 5 — `lib/components/fabro-sandbox`**
- `src/error.rs`: kube-error → `crate::Error` context helpers (mirror `docker_connect`).
- `src/managed_labels.rs`, `src/clone_source.rs`, `src/from_environment.rs`: extend `#[cfg(any(...))]` gates with `kubernetes`.
- `src/sandbox.rs`: move Docker's private `clean_bash_env_entries` here as `pub(crate)` (BASH_ENV scrub shared by both providers); extend remote-helper cfg gates.
- `src/kubernetes/*` (new, above).
- `src/sandbox_spec.rs`: `SandboxSpec::Kubernetes {config, github_app, run_id, clone_origin_url, clone_branch, clone_tag, clone_commit_sha, agent_token_key}`; `build()`; `to_run_sandbox_instance` mirroring Docker's layout metadata.
- `src/provider.rs`: `SandboxCreateSpec::Kubernetes`.
- `src/from_environment.rs`: `kubernetes_config_from_environment`; agent image default `ghcr.io/fabro-sh/fabro-sandbox-agent:<workspace version>`, overridable via `FABRO_KUBERNETES_AGENT_IMAGE`.
- `src/reconnect.rs`: replace the per-provider credential params with `ReconnectCredentials {daytona_api_key, kubernetes_agent_key}` (mechanical refactor of call sites: `server.rs` ×2, `run_files.rs`, handler paths); Kubernetes arm → `KubernetesSandbox::reconnect` (missing pod → "workspace was ephemeral" error).
- `src/terminal.rs`: Kubernetes arm → "Kubernetes sandboxes do not support terminals".
- `src/lib.rs`: module, re-exports, cfg gates.

**Step 6 — agent binary, image tooling, release wiring**
- Agent crate + Dockerfile + `cargo dev agent-image` (above).
- `.github/workflows/release.yml` and `nightly.yml`: extend the existing docker job (or parallel job) to build+push `ghcr.io/fabro-sh/fabro-sandbox-agent` (amd64+arm64) from the agent Dockerfile, so the default ref resolves on release.
- `lib/foundation/fabro-static/src/env_vars.rs`: add `KUBERNETES_SERVICE_PORT` (HOST exists at line 125); add `FABRO_KUBERNETES_AGENT_IMAGE` to the known-vars list (non-secret).

**Step 7 — `lib/apps/fabro-server`**
- `src/spawn_env.rs`: add `EnvVars::KUBERNETES_SERVICE_HOST` + `EnvVars::KUBERNETES_SERVICE_PORT` to `WORKER_ENV_ALLOWLIST` with the same "must survive `env_clear()`" comment as the AWS block (in-cluster kube discovery is env-gated; workers construct sandboxes); update `worker_allowlist_is_fail_closed` and the exhaustive allowlist test.
- `src/server.rs`: `build_sandbox_provider_registry` registers when enabled **and** `kube::Client::try_default()` succeeds (Daytona-style gating, `debug!` on skip); reconnect call sites pass `ReconnectCredentials` with the vault `SESSION_SECRET` as the agent key; `vault_secret(EnvVars::SESSION_SECRET)` at spec build for preflight.
- `src/run_manifest.rs`: `preflight_sandbox_spec` Kubernetes arm (`skip_clone = true`, agent key from vault); `clone_disabled_for_provider` arm; Daytona-only error message generalized.
- `src/server/handler/sandbox.rs`: ssh-access arm → CONFLICT "does not support access commands" (Local precedent); VNC/preview already generic `!= Daytona`.
- `src/server/handler/runs.rs` (~1106–1119): Kubernetes in the "no provider-side build" arm.

**Step 8 — `lib/components/fabro-workflow/src/operations/start.rs`**
- `SandboxSpec::Kubernetes` construction arm + `resolve_kubernetes_config` (secrets-aware; agent key read from the CLI/server secret store, mirroring the Daytona API-key flow); tests mirroring the Docker/Daytona set (dry-run coercion, none-target, clone defaults, missing-session-secret error).

**Step 9 — API contract**
- `docs/public/api-reference/fabro-api.yaml`: `SandboxProviderKind` + both `EnvironmentProvider` enums gain `kubernetes`; `ServerSandboxProvidersSettings` `required` + property.
- `cargo build -p fabro-api` regenerates types (`SandboxProviderKind` stays replaced with `fabro_types::` via existing `with_replacement`); extend `lib/foundation/fabro-api/tests/` round-trips where variants are enumerated.
- `cd lib/packages/fabro-api-client && bun run generate`.

**Step 10 — web UI**
- `apps/fabro-web/app/lib/environment-providers.ts`: `KUBERNETES` in `CREATABLE_PROVIDERS`.
- `apps/fabro-web/app/components/environment-form.tsx`: Kubernetes hides dockerfile source (image-ref-only hint) and locks network mode to `allow_all`.
- `apps/fabro-web/app/routes/settings-sandboxes.tsx`: provider display if it enumerates kinds (verify at edit time).

**Step 11 — docs**
- `docs/public/execution/environments.mdx`: Kubernetes section (image-only, allow_all-only, bash+git image contract, agent sidecar + pullability, stop-deletes-pod ephemerality — completed-run sandboxes are not browsable, unlike Docker, RBAC list, ambient kubeconfig incl. the `HOME`-based worker path).
- `docs/public/administration/sandboxing.mdx`: four providers; RBAC + agent image notes.
- Changelog entry.

## Verification

- `cargo nextest run -p fabro-types -p fabro-config -p fabro-environment -p fabro-store -p fabro-sandbox -p fabro-workflow -p fabro-server -p fabro-api`, then `--workspace`.
- `cargo +nightly-2026-04-14 fmt --check --all` and `clippy --workspace --all-targets -- -D warnings`.
- `cargo insta pending-snapshots` reviewed before accepting (enum additions may touch settings/install snapshots).
- `cd apps/fabro-web && bun test && bun run typecheck`; regenerated client diff reviewed.
- New unit tests (cluster-free): token-derivation stability (derive ≡ re-derive across processes, rejects wrong key), config mapping, validation rules (dockerfile/network/labels), pod-manifest builder (labels/volumes/resources/env/override), pod→`SandboxInfo`, spec→record metadata, workflow spec construction, **allowlist fail-closed test updated to assert the `KUBERNETES_*` vars pass and others don't**, agent protocol (spawn/stdin/terminate vs local processes, auth-token check).
- **Manual, because no cluster CI exists**: `cargo dev agent-image` → `kind load docker-image` (override `FABRO_KUBERNETES_AGENT_IMAGE` to the loaded tag) → create a Kubernetes environment via `/api/v1/environments` → run a workflow (clone, exec, file edit, git push) → verify preflight, `/api/v1/sandboxes` list/get, terminal endpoint returns the explicit unsupported error, run deletion deletes the pod, `preserve` leaves it, **restart the server mid-run and reconnect** (token re-derivation), and a dry-run coerces to local. Live-cluster integration tests added `#[ignore]`d per `docs/internal/testing-strategy.md`.

## Deliberately not doing

- No NetworkPolicy enforcement; no dockerfile builds; no PTY terminal, preview URLs/VNC, or install-wizard entry (human's answers — explicit "not supported" errors).
- No cluster settings schema beyond `server.sandbox.providers.kubernetes.enabled` (ambient-only, human's answer); no multi-cluster; no CRDs/operators; no kind-in-CI.
- No PVCs — `stop_on_terminal` deletes the pod; completed-run sandboxes are not browsable (documented consequence, surfaced and accepted in the answers round).
- No `auto_stop` idle reclamation (Docker precedent); no changes to Local/Docker/Daytona behavior; `KUBECONFIG` env still does not reach workers (ambient path is `~/.kube/config` via `HOME` or in-cluster SA via the two allowlisted discovery vars).