Exploration complete. Here is my analysis.

---

## 1. Repository structure and where this change lands

**Language/tooling:** Rust workspace (edition 2021, ~50 crates in `lib/apps`, `lib/components`, `lib/foundation`), plus a TypeScript React SPA (`apps/fabro-web`, Bun-bundled), an OpenAPI-first API contract (`docs/public/api-reference/fabro-api.yaml` → progenitor Rust types + generated TS client), and docs (`docs/public` user-facing, `docs/internal` strategy docs). Tests via `cargo nextest`; strict clippy/fmt on a pinned nightly.

**Fabro is an AI workflow orchestration platform.** Runs execute agent tools inside *sandboxes*. There are exactly three sandbox providers today: `local` (host exec), `docker` (bollard-managed sibling containers — the default), and `daytona` (cloud VMs via a vendored SDK). A Kubernetes provider would be the fourth, and its blast radius spans:

| Area | Files |
|---|---|
| **Provider core** | `lib/components/fabro-sandbox/` — `sandbox.rs` (`Sandbox` trait, ~40 methods + shared remote helpers cfg'd `any(feature = "docker", "daytona")`), `docker.rs` (3,101 lines) and `daytona/mod.rs` (5,093 lines) as templates, `sandbox_spec.rs` (`SandboxSpec` enum), `provider.rs` (`SandboxProvider` inventory trait, `SandboxCreateSpec`), `details.rs`, `reconnect.rs`, `terminal.rs`, `managed_labels.rs` (`sh.fabro.managed=true`), `clone_source.rs`, `from_environment.rs`, `error.rs`, `Cargo.toml` (new `kubernetes` feature) |
| **Types** | `lib/foundation/fabro-types/` — `sandbox_provider.rs` (`SandboxProviderKind`), `settings/run.rs` (`EnvironmentProvider`), `settings/server.rs` (`ServerSandboxProvidersSettings`) |
| **Config** | `lib/foundation/fabro-config/` — provider parsing + `validate_provider_capabilities` (which image/network/resource combos are legal per provider) |
| **Server** | `lib/apps/fabro-server/` — `server.rs` `build_sandbox_provider_registry()`, `run_manifest.rs` preflight, `handler/sandbox.rs` (previews/services), `install.rs` (install-time provider picker), `diagnostics.rs` |
| **Workflow** | `lib/components/fabro-workflow/` — `operations/start.rs` builds the `SandboxSpec` per run (vault API-key lookup pattern exists for Daytona) |
| **Contract/UI/docs** | `fabro-api.yaml` (enums: `SandboxProviderKind`, `EnvironmentProvider`, `ServerSandboxProvidersSettings`, `InstallSandboxInput`), `apps/fabro-web/app/routes/settings-sandboxes.tsx`, `docs/public/administration/sandboxing.mdx`, `docs/public/execution/environments.mdx`, `lib/foundation/fabro-static/src/env_vars.rs` |

No `kube`/`k8s-openapi` dependency exists today; `Cargo.lock` has none.

## 2. Goal restated in this codebase's terms

Add a fourth sandbox provider — `kubernetes` — that executes run workloads in a Kubernetes cluster instead of a Docker daemon or Daytona VM. Concretely:

1. A new `KubernetesSandbox` implementing the `Sandbox` trait (exec/streaming-exec/stdio-process, file read/write/upload/download, glob/grep/walk, lifecycle initialize/activate/start/stop/delete/cleanup, git setup/push with credential refresh, terminal attach), most likely by creating one **Pod per run** with the run's container image and exec'ing into it — reusing the provider-neutral remote helpers (`/bin/bash` probe, `BASH_ENV` scrubbing, `find`-based walk, shell-quoting, managed labels, clone contract).
2. A new `KubernetesSandboxProvider` implementing the inventory trait (`list`/`get`/`create`/`delete` over managed, label-selected resources) registered in `build_sandbox_provider_registry()` when enabled and configured.
3. Threading `Kubernetes` through `SandboxProviderKind` / `EnvironmentProvider` / server provider policy / config validation / run-start spec construction / reconnect / details / OpenAPI enums / TS client / web UI / docs — behind a new `kubernetes` cargo feature, following the exact pattern Docker and Daytona established.

## 3. What I know for certain

- **Two provider levels exist and both are needed**: the `Sandbox` trait (per-run execution, built from `SandboxSpec` in `fabro-workflow` `operations/start.rs`) and the `SandboxProvider` trait (inventory for `GET /api/v1/sandboxes`, aggregated fail-soft by `SandboxProviderRegistry`).
- **The clone contract is well-defined and provider-neutral**: clone-based providers clone GitHub origins into `/workspace` with `/repos/<owner>/<repo>` + symlink layout, `fabro/run/<run_id>` branch, default depth 100, optional exact SHA/tag (no branch-head fallback), `skip_clone` → empty workspace, non-GitHub origins rejected. Shared helpers in `clone_source.rs` are feature-gated `any(docker, daytona)` and would gain `kubernetes`.
- **The execution contract**: `/bin/bash` required (no `sh` fallback), non-login, `BASH_ENV` blanked, probe-marker validation, `exec_command` + `exec_command_streaming` + `spawn_stdio_process`, redacted output tails.
- **Managed-resource ownership** is enforced via `sh.fabro.managed=true` + `sh.fabro.run_id` labels; providers refuse to delete unlabeled resources.
- **Git push credentials** flow through `PushCredentialState`/`InstallationTokenSource` with retry classification (`git_retry.rs`) — this machinery is provider-shared, not Docker-specific.
- **Configuration model**: `settings.toml` → `[server.sandbox.providers.<p>]` enablement; per-run `[environments.<slug>]` with `provider`, `image.docker`/`image.dockerfile`, `resources` (cpu/memory/disk), `network` (allow_all/block/cidr_allow_list), `lifecycle`, labels, env. `fabro-config` validates provider capabilities (e.g. local can't enforce CIDR lists; Daytona accepts image *or* dockerfile, not both).
- **Feature-gating pattern**: `fabro-sandbox` exposes `docker`, `daytona` features; `fabro-server` enables both; `fabro-workflow` enables `daytona` (+docker in dev-deps); `fabro-cli` enables `daytona`. Unimplemented feature branches `bail!` with "<provider> sandbox support is not enabled".
- **API contract is generated, not hand-written**, in TS; enum additions require the YAML → `cargo build -p fabro-api` → `bun run generate` chain, and `with_replacement` keeps Rust/API types identical.
- **Secrets**: provider credentials come from the vault (`EnvVars::DAYTONA_API_KEY` pattern) with `fabro-static` registry entries, install-time persistence, and diagnostics checks.
- **Test strategy docs exist** and must be consulted before adding tests (`docs/internal/testing-strategy.md`, `events-strategy.md`, `logging-strategy.md`, `error-handling-strategy.md`).
- **Nothing Kubernetes-related exists anywhere in the repo today** — this is greenfield.

## 4. Genuinely ambiguous (two reasonable engineers would build differently)

1. **Cluster resource primitive.** One long-running Pod per run is the natural fit for `exec`, but reasonable alternatives: a Pod + PVC (workspace persistence across restarts), a Job, or an EphemeralContainer injected into a pre-provisioned worker Pod. Start/stop/autostop semantics differ radically across these (stop = delete pod? scale to 0? checkpoint to a volume?).
2. **Kubernetes client choice.** `kube-rs` (`kube` + `k8s-openapi`) gives exec/websocket support but is a large dependency tree; the repo has precedent for lean, hand-rolled clients over `fabro-http`/reqwest (see the deliberate avoidance of `aws-sdk-bedrockruntime`) — but hand-rolling the SPDY/WebSocket exec subprotocol (stdin/stdout/stderr/resize channels) is substantial work (Daytona's terminal already pulls `tokio-tungstenite`).
3. **Cluster connection & config surface.** In-cluster ServiceAccount vs kubeconfig file vs explicit `KUBERNETES_*` env vars; which of namespace, context, nodeSelector, tolerations, imagePullSecrets, securityContext, serviceAccount become `[environments]` settings vs `[server.sandbox.providers.kubernetes]` operator settings vs hardcoded. Also whether settings live in the vault (Daytona pattern) or in `settings.toml`.
4. **Provider string and enum naming.** `"kubernetes"` vs `"k8s"` — this is wire-visible in four enums plus config parsing, so it must be decided up front.
5. **Resource mapping fidelity.** `resources.cpu`/`memory` map to requests/limits — but requests-vs-limits split is unstated; `resources.disk` has no direct analog (ephemeral-storage limit? PVC size? ignored-with-warning like Docker ignores it?). Network modes: `block` could be a per-run NetworkPolicy (default-deny egress) or an unsupported-hard-error (like Local); `cidr_allow_list` similarly needs per-run NetworkPolicy generation.
6. **Image handling.** Kubernetes pulls but never builds. Does `image.dockerfile` get rejected (provider-capability validation), or supported via an in-cluster build step (Kaniko/BuildKit — a big scope expansion)? Also: is a pull-through registry/mirror or an imagePullSecret part of v1?
7. **Preview URLs / SSH / VNC.** `get_preview_url` is currently Daytona-only (ports 3000–9999); `ssh_access_command` is Docker-only. For Kubernetes: `kubectl exec -it <pod> -- ...` is trivially constructible; preview URLs would need a routing/ingress story that doesn't exist. Which of these are in scope?
8. **Terminal support depth.** Daytona has a real WebSocket terminal; Docker terminals go through exec streams. Kubernetes `exec` requires the v4 channel WebSocket protocol with resize support — worth it, or v1 ships without embedded terminals?
9. **Install flow and UI scope.** Is `kubernetes` selectable in the browser install wizard (currently `enum: [docker, daytona]`), only in `settings.toml`, or both? Does the web `SandboxProviderId` union and settings-sandboxes page need a third card?
10. **Scope of "v1".** Minimum: run execution + inventory + reconnect. Everything else (terminals, previews, diagnostics card, demo mode entries, docs pages, install wizard, lifecycle autostop) is individually optional and each omission changes the enum/config/API surface.
11. **Reconnect identity.** `RunSandboxInstance.runtime.id` for Kubernetes = pod name (stable, dies with pod) vs a PVC-backed regenerate-on-reconnect design. The existing Docker/Daytona reconnect contract assumes the resource survived.
12. **Testing strategy.** No `kube` twin exists (twins exist for openai/github only). Options: httpmock-faked REST endpoints for the API server, a `kind`/`minikube`-gated `#[e2e_test]` live suite (Daytona pattern), or both — with different maintenance costs.
13. **Multi-tenancy/security posture.** Docker's documented model is "trusted, single-tenant, host-root-equivalent." Kubernetes RBAC could actually offer real isolation — does the provider target the same trusted-single-tenant assumption (cluster-admin-ish pod CRUD in one namespace) or attempt per-run ServiceAccounts/security contexts? This changes both code and docs materially.