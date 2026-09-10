The implementation is complete. Here's the summary of what I changed, file by file.

## What was built

A **`kubernetes` sandbox provider** (pod-per-run, clone-based, agent-sidecar exec transport) wired end-to-end through the stack, following the approved plan.

## New files

| File | Contents |
|---|---|
| `lib/components/fabro-sandbox/src/kubernetes/mod.rs` | `KubernetesSandbox` (full `Sandbox` trait: exec, streaming exec with tracked-process terminate-on-timeout, stdio processes, exec-based file ops/walk/grep, git setup/push/credential refresh via the shared `setup_git_via_exec`/`git_push_via_exec` helpers), `KubernetesSandboxOptions`, `derive_agent_token` (HMAC-SHA256 over session secret + pod name), pod-ready wait, clone machinery mirroring Docker's on the shared `clone_source` builders. Unit tests: token stability/key-binding, clone-command shape, BASH_ENV scrubbing. |
| `.../kubernetes/pod.rs` | Pure pod-manifest builder (init container installs the agent + prepares `/workspace`//`/repos` 0777; main container overrides command to the agent; managed+user labels, resources requests=limits, emptyDir volumes). Unit-tested without a cluster. |
| `.../kubernetes/agent_client.rs` | HTTP/1.1 client over kube port-forward streams (`TokioIo` + hyper), JSON-lines decoding, spawn/stdin/terminate, `X-Fabro-Process-Id` header. |
| `lib/components/fabro-sandbox/src/provider/kubernetes.rs` | `KubernetesSandboxProvider` — list/get/delete with managed-label safety, registry-parity `create`. |
| `lib/apps/fabro-sandbox-agent/` | The in-pod agent (axum + tokio + libc): `/exec`, `/processes`, stdin/terminate endpoints, bearer auth with constant-time compare, PID-1 waitpid reaper, `self-install` (distroless-compatible), `healthz` probe. Dockerfile (distroless, staged-binary layout matching the release pipeline). |
| `lib/foundation/fabro-dev/src/commands/agent_image.rs` | `cargo dev agent-image` mirroring `docker_build` (zigbuild, `--compile-only`, `--dry-run`). |
| `docs/public/changelog/2026-09-10.mdx` | Changelog entry. |

## Modified files (key ones)

- **fabro-types**: `SandboxProviderKind::Kubernetes`, `EnvironmentProvider::Kubernetes`, `ServerSandboxProvidersSettings.kubernetes`, `is_clone_based` extended.
- **fabro-config**: Kubernetes validation (rejects `image.dockerfile`, non-`allow_all` network, invalid label keys/values) + 4 new tests; provider layer + resolve.
- **fabro-environment / fabro-store**: Kubernetes seed environment; `sandbox_plan` exposes image for Kubernetes like Docker.
- **fabro-sandbox**: `SandboxSpec::Kubernetes`, `SandboxCreateSpec::Kubernetes`, `ReconnectCredentials` struct (per plan), pod→`SandboxInfo` details mapping (includes unit tests), terminal returns explicit "not supported", shared `scrub_bash_env_entries` moved into `sandbox.rs`, cfg gates extended, error variants with boxed kube errors.
- **fabro-server**: `KUBERNETES_SERVICE_HOST/PORT` in the worker allowlist (+ fail-closed test), registry registers Kubernetes only when the ambient cluster resolves (`debug!` on skip), preflight spec + capability warnings, ssh-access 409 arm, reconnect credentials threaded through `server.rs`/`run_files.rs`/handlers.
- **fabro-workflow**: Kubernetes spec construction with secrets-aware env resolution; the worker re-derives the agent key from `SESSION_SECRET` env or the `FABRO_STORAGE_ROOT` server env file; new test mirroring the Daytona `none`-target one.
- **fabro-install**: writes `kubernetes.enabled = false` explicitly (opt-in; the wizard can't validate cluster access).
- **OpenAPI + generated clients**: `kubernetes` added to both enums and providers settings; Rust types regenerated (`cargo build -p fabro-api`), TS client regenerated; round-trip tests extended.
- **Web UI**: Kubernetes in `CREATABLE_PROVIDERS`, image-reference-only source (no dockerfile), network blocking hidden, sandboxes settings row.
- **Docs**: Kubernetes section in `execution/environments.mdx`, provider table column, `administration/sandboxing.mdx` updated.
- **`.github/workflows/release.yml`**: compiles the agent alongside the CLI for musl targets, stages it into the Docker context, builds/pushes/attests `ghcr.io/fabro-sh/fabro-sandbox-agent`.

## Deviations from the plan (and why)

1. **k8s-openapi 0.28 API differences**: `Pod.metadata` is non-`Option` and maps are `BTreeMap`s — adapted builders/mappings accordingly.
2. **`sandbox_details()` implements Kubernetes** (get pod → neutral details) instead of deferring to the catch-all error — strictly better UX at trivial cost; inventory behavior unchanged.
3. **Worker-side agent key** resolves `SESSION_SECRET` from process env first, then the `FABRO_STORAGE_ROOT` server env file (the plan's "FABRO_HOME vault access" mechanism made concrete — the SQLite vault doesn't hold bootstrap secrets).
4. **Env-file path**: server env resolution uses `FABRO_STORAGE_ROOT` (which is allowlisted); install writes `kubernetes.enabled = false` rather than leaving it default-enabled.

## Verification

`cargo +nightly fmt --check` clean, `clippy --workspace --all-targets -D warnings` clean, `insta` no pending snapshots, and nextest green: types+config+environment+store+sandbox+api+install+agent **1448+275+316+4**, workflow **1445**, server **903**, CLI **995**. Web: typecheck clean; the 25 failing web tests fail identically at baseline (container-timing sensitive) and none touch changed files. Two failures hit during verification (`get_graph_returns_svg`, CLI server-spawning tests) were traced to environment limits — OOM-killed linker and a full disk — and pass after building the CLI binary and freeing space. Live-cluster behavior is covered by the documented manual `kind` checklist; no cluster exists in CI per the plan.