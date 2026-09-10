All integration points are now verified. Here is the implementation plan.

---

# Implementation plan: `kubernetes` sandbox provider (core tier)

Authoritative inputs: Q1 answer = **fourth sandbox provider** (one Pod per run, exec'd into by the server, no fabro-on-k8s packaging); Q2 answer = **core only** (no embedded terminal, no install-wizard entry, no preview URLs/VNC, `image.dockerfile` rejected). Sweep decisions carry over: kube-rs client, exec-based cloning inheriting shared git machinery, NetworkPolicy enforcement for `block`/`cidr_allow_list`, `stop()` = delete pod, kube-standard config inference (no new settings keys beyond `enabled`), provider string `kubernetes`.

## Design specifics the plan is built on

- **Pod-per-run**: name `fabro-<run_id>` (or `fabro-<uuid>` for preflight), labels `sh.fabro.managed=true`, `sh.fabro.run_id` (when present), plus a unique `sh.fabro.sandbox=<pod-name>` label used as the NetworkPolicy selector. `command: ["sleep","infinity"]`, `restartPolicy: Never`, resources requests=limits (cpu cores, memory bytes, `disk` → `ephemeral-storage` quantity), default image `buildpack-deps:noble` (satisfies the `/bin/bash` contract).
- **Exec transport**: kube exec/attach WebSocket streams. The k8s exec API has **no per-exec env and no working-dir parameters**, so commands are wrapped: `cd <cwd> && env K=V … BASH_ENV= /bin/bash -c '<shell_quote(cmd)>'` (same approach Daytona uses). The API also returns **no exit code**, so the wrapper appends a trailing `__FABRO_RC__=<n>` marker on stdout that the reader parses and strips before delivering bytes to callers/callbacks. Timeout/cancel = client-side deadline + follow-up kill exec, reported as `CommandTermination::TimedOut`/`Cancelled`.
- **File transfer**: no pod copy API exists — upload = exec `tar -x` fed via stdin stream; download = exec `tar -c` read from stdout (same technique `kubectl cp` uses). read/write/glob/grep/walk reuse the shared remote command builders already cfg-gated for docker/daytona.
- **Network**: `allow_all` = no policy; `block` = per-pod NetworkPolicy `policyTypes:[Egress], egress: []`; `cidr_allow_list` = egress `ipBlock` entries plus kube-dns egress (UDP/TCP 53) so names resolve. Policies carry an `ownerReferences` to the Pod so the API server GCs them with it. CNI-enforcement caveat documented.
- **Clone/push**: identical to Docker — `initialize` clones via exec using `clone_source`/`git_retry`; `setup_git`/`git_push_ref`/`refresh_push_credentials` ride the shared `push_credentials` machinery (`PushCredentialState` held like `DockerSandbox` holds it).
- **Connection**: `kube::Config::infer()` (in-cluster SA → `KUBECONFIG` → `~/.kube/config`); namespace = inferred default namespace. No vault secret, no new env vars, no new `[server.sandbox.providers.kubernetes]` keys beyond `enabled`. Registry registration is sync-safe via a lazily-built `OnceCell<kube::Client>`; an unresolvable config surfaces as a provider error in the fail-soft inventory `meta`.

## Files, changes, and order

The variant addition breaks every exhaustive `match` downstream, so the order is: dependencies → variant + mechanical arms + stub provider (workspace compiles, k8s paths return "not implemented") → real internals → wiring → contract/UI/docs → tests.

### Step 1 — Dependencies
1. **`Cargo.toml`** (workspace root): add `kube` and `k8s-openapi` (pinned cluster-version feature, e.g. latest stable with `v1_3x`) to `[workspace.dependencies]`. `Cargo.lock` updates are part of the change (CI builds `--locked`). Heavy tree accepted deliberately — streaming exec is mandatory, hand-rolling the v4 WebSocket subprotocol is strictly worse.
2. **`lib/components/fabro-sandbox/Cargo.toml`**: new feature `kubernetes = ["dep:kube", "dep:k8s-openapi"]`.

### Step 2 — Foundation types (`lib/foundation/fabro-types`)
3. **`src/sandbox_provider.rs`**: add `Kubernetes` variant (serde/strum lowercase `kubernetes`); include it in `is_clone_based()`; extend the parse/display tests.
4. **`src/settings/run.rs`**: `EnvironmentProvider::Kubernetes` + `is_clone_based` arm + `From<EnvironmentProvider> for SandboxProviderKind` arm.
5. **`src/settings/server.rs`**: `ServerSandboxProvidersSettings.kubernetes: ServerSandboxProviderSettings` + `for_provider()` arm.
6. **`tests/sandbox_model_serde.rs`**: `"kubernetes"` parse/serialize assertion.

### Step 3 — Config layer (`lib/foundation/fabro-config`)
7. **`src/layers/server.rs`**: optional `kubernetes` provider entry in the sandbox providers layer.
8. **`src/resolve/server.rs`**: resolve it (default `enabled = true`, matching siblings).
9. **`src/resolve/environment.rs`** — `validate_provider_capabilities`: new `Kubernetes` arm rejecting `image.dockerfile` (Inline or Path) with a clear reason ("kubernetes environments require image.docker; building Dockerfiles in-cluster is not supported"); all three network modes accepted.
10. **`src/tests/resolve_run.rs` / `src/tests/resolve_server.rs`**: tests for the dockerfile rejection and `providers.kubernetes.enabled = false`.

### Step 4 — Sandbox crate skeleton (workspace compiles again)
11. **`src/sandbox.rs`**: widen cfg gates on shared remote helpers to `any(feature = "docker", feature = "daytona", feature = "kubernetes")`: `REMOTE_BASH`, `REMOTE_WALK_TIMEOUT_MS` (lines ~34/38) and `resolve_path`, `join_sandbox_path`, `build_remote_walk_command`, `parse_remote_walk_output` (lines ~1554–1611).
12. **`src/managed_labels.rs`**: widen `is_managed` cfg to include `kubernetes`. Do **not** touch `for_run`/`merge_for_run` — the k8s module composes its own label map (managed + run_id + unique `sh.fabro.sandbox`) from the exported constants, leaving Docker behavior and tests untouched.
13. **NEW `src/kubernetes.rs`** (starts as stub, filled in Step 5): `KubernetesSandboxOptions` (`image`, `env_vars`, `memory_limit: Option<i64>`, `cpu: Option<i32>` cores, `ephemeral_storage_limit: Option<i64>`, `network: KubernetesNetworkMode`, `clone_depth`, `skip_clone`; default image `buildpack-deps:noble`) and `KubernetesSandbox` with constructor/validation mirroring `DockerSandbox::new` (clone-spec validation via `clone_source::decide_clone`, `PushCredentialState` from github_app + origin).
14. **`src/sandbox_spec.rs`**: `SandboxSpec::Kubernetes { config, github_app, run_id, clone_origin_url, clone_branch, clone_tag, clone_commit_sha }`; `provider_name()` arm; `build()` arm; `to_run_sandbox_instance()` arm with `workspace_root=/workspace`, `repos_root=/repos` (module-local `WORKING_DIRECTORY`/`REPOS_ROOT` consts, matching how docker/daytona each own theirs).
15. **`src/provider.rs`**: `SandboxCreateSpec::Kubernetes` variant.
16. **NEW `src/provider/kubernetes.rs`**: `KubernetesSandboxProvider` — `list()` (label selector on `sh.fabro.managed`), `get()` (404→`Ok(None)`, managed-label guard), `create()` (new + initialize + re-read), `delete()` (managed-label refusal guard), lazy `OnceCell` client.
17. **`src/details.rs`**: `Kubernetes` dispatch arm + `mod kubernetes` mapping Pod phase → `SandboxState` (Pending→Starting, Running→Running, Succeeded/Failed→Stopped/Error, else Unknown), nodeName→region, container image, creation timestamp.
18. **`src/reconnect.rs`**: `Kubernetes` arm — `KubernetesSandbox::reconnect(pod_name, repo_cloned, working_directory, origin, branch, run_id)`; no API-key plumbing needed.
19. **`src/terminal.rs`**: `Kubernetes` arm returning a "Kubernetes sandboxes do not support embedded terminals" error (mirrors the Local arm) — per the Q2 answer.
20. **`src/error.rs`**: kube error mapping (a `kubernetes_connect`-style context constructor alongside `docker_connect`).
21. **`src/from_environment.rs`**: `kubernetes_config_from_environment` + `_with_secrets` mirroring the Docker pair (env resolution, network mapping to `KubernetesNetworkMode`, cpu/memory/disk quantities, clone depth/skip).
22. **`src/lib.rs`**: module, feature gate, and re-exports (`KubernetesSandbox`, `KubernetesSandboxOptions`, `KubernetesSandboxProvider`).

### Step 5 — Provider internals (`src/kubernetes.rs`, the bulk of the work)
- Client bootstrap + pod create/read/wait (poll to Ready with terminal-failure detection: `ErrImagePull`/`ImagePullBackOff`/`CrashLoopBackOff` → `InitializeFailed`-style error with remediation text, mirroring Daytona's poll-to-Active pattern; pull timeout allowance ~5m).
- Bash probe + `/tmp/fabro/runtime` mkdir in `initialize` (both required by existing contracts).
- Clone flow via exec (`clone_source::decide_clone`, shallow fetch, exact-SHA/tag handling — copy `DockerSandbox`'s clone step logic, which is provider-neutral shell).
- Exec layer: wrapper builder (cwd + `env` + `BASH_ENV=` + `bash -c`), `__FABRO_RC__` sentinel writer/parser with strip-before-delivery, streaming multiplex into `OutputCaptureBuffer`/callbacks, timeout/cancel kill path; `exec_command`, `exec_command_streaming`, `spawn_stdio_process` (needed by MCP servers — this is core, not "interactive tier").
- File ops: `read_file(_bytes)`, `write_file`, `delete_file`, `file_exists`, `list_directory`, `grep`, `walk_files` (shared builders), `download/upload_file_to_local` (tar-over-exec).
- Lifecycle: `activate`/`start` verify Running (idempotent, no restart), `stop()` deletes the pod, `delete`/`cleanup` delete pod (ownerRef GCs the policy) with idempotent 404 handling.
- Git: `setup_git` (via `setup_git_via_exec`), `git_push_ref`/`refresh_push_credentials` via `push_credentials` machinery, `resume_setup_commands`.
- Metadata: `working_directory`, `platform`/`os_version` (via `uname` exec, cached), `sandbox_info` = `kubernetes:<namespace>/<pod>`, `runtime_directory`, `ssh_access_command` = `kubectl exec -it <pod> -n <ns> -- sh -lc 'cd <wd> && exec sh -l'`, `origin_url`.

### Step 6 — Workflow + server wiring
23. **`lib/components/fabro-workflow/Cargo.toml`**: add `"kubernetes"` to the fabro-sandbox features (line 32), mirroring daytona.
24. **`src/operations/start.rs`**: `SandboxProviderKind::Kubernetes` arm (~line 532) building the spec via `kubernetes_config_from_environment_with_secrets(resolved, secret_lookup)` with `skip_clone |= clone_source.skip_clone` and the standard clone fields.
25. **`lib/apps/fabro-server/Cargo.toml`**: add `"kubernetes"` to fabro-sandbox features (line 37) — this puts it in the shipped binary (release builds `-p fabro-cli`, which unifies through the server dependency).
26. **`src/server.rs`** — `build_sandbox_provider_registry()` (~line 2344): register `KubernetesSandboxProvider` when `provider_settings.kubernetes.enabled`.
27. **`src/run_manifest.rs`**: `preflight_sandbox_spec` Kubernetes arm (skip_clone=true, line ~925 match); `clone_disabled_for_provider` gains `Kubernetes` (line ~690); `environment_capability_warnings` Kubernetes arm (line ~714) — warn "kubernetes provider ignores cwd" and "ignores lifecycle.auto_stop"; labels and disk are honored (no warning).
28. **`src/server/handler/runs.rs`** — `validate_intent_environment` (~line 1104): `image_incompatible` adds `Kubernetes => false` (config layer enforces dockerfile rejection); update the two "Docker or Daytona environment" detail strings to name all three clone-based providers.
29. **`src/server/handler/sandbox.rs`**: access-command match (~line 420) gains a `Kubernetes` arm mirroring the Docker shape (it consumes `ssh_access_command()`); the `== Daytona` preview/VNC guards need no change and keep k8s excluded by construction.
30. **`src/diagnostics.rs`**: `check_kubernetes_sandbox` card — policy check, config inference, `client.apiserver_version()` reachability probe, remediation text on failure; wired into the existing `tokio::join!` (line ~97).
31. **`src/server/tests.rs`**: provider-policy test analog to the Daytona one (line ~1370) for `kubernetes`.

### Step 7 — API contract
32. **`docs/public/api-reference/fabro-api.yaml`**: `kubernetes` in `SandboxProviderKind` (line ~12948), `EnvironmentProvider` (line ~15042), and `ServerSandboxProvidersSettings` required+properties (line ~14456). `InstallSandboxInput`/`Summary` stay `docker|daytona` (line ~6590) per the Q2 answer.
33. Regenerate: `cargo build -p fabro-api` (progenitor; `with_replacement` already maps `SandboxProviderKind` to `fabro_types`, so no build.rs change — type identity and JSON parity tests keep holding).
34. **`lib/packages/fabro-api-client`**: `bun run generate` + typecheck.

### Step 8 — Web + docs
35. **`apps/fabro-web/app/routes/settings-sandboxes.tsx`**: add `"kubernetes"` to `SandboxProviderId` (line 20) and a provider card entry in the list (lines ~55–64).
36. **`docs/public/administration/sandboxing.mdx`**: four-provider sentence; Kubernetes subsection — connection inference (in-cluster SA / kubeconfig), required RBAC (pods + exec + networkpolicies in one namespace), CNI-enforcement caveat for network modes, `stop` = pod deletion.
37. **`docs/public/execution/environments.mdx`**: Kubernetes provider section — image required/default, dockerfile rejected, resources mapping (disk → ephemeral-storage), labels → pod labels, network modes, clone-based contract (inherits the Docker description), no embedded terminal.
38. **NEW `docs/public/changelog/2026-09-10.mdx`**: dated entry following the existing front-matter format; notes the new provider, the downgrade caveat (run records with `provider: "kubernetes"` won't deserialize on older binaries — same property Daytona's addition had).

### Step 9 — Tests (written with their steps, run in Step 10)
- Unit (`fabro-sandbox`): label-map composition (incl. reserved-label override), pod name generation, env-wrapper builder quoting, `__FABRO_RC__` sentinel parser (success/timeout/cancel/marker-in-output cases), NetworkPolicy object construction for all three modes, pod-spec quantities, phase→state mapping.
- httpmock-backed REST tests: point a `kube::Config` at the mock server (plain HTTP, no auth) covering list/get/delete label filtering, managed-label refusal, 404 idempotence. If kube's custom-config path fights us, fallback: build `kube::Client` from a mock tower service (kube's documented testing pattern) — flagged honestly as the one place the harness may need adjustment.
- Live-gated **NEW `lib/components/fabro-sandbox/tests/kubernetes_live.rs`**: mirrors `docker_streaming.rs` — `#[ignore = "requires a Kubernetes cluster…"]`, env-gated skip when no config: initialize+probe, exec exit codes, streaming, tar round-trip, cleanup removes pod+policy.
- `fabro-types`/`fabro-config` tests as listed in Steps 2–3.

## Verification

1. `cargo build --workspace` after every step (the stub-first ordering keeps this green throughout).
2. `cargo nextest run -p fabro-types -p fabro-config -p fabro-sandbox -p fabro-workflow -p fabro-server`, then full `cargo nextest run --workspace`.
3. `cargo +nightly-2026-04-14 fmt --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
4. API conformance: `cargo nextest run -p fabro-server` (spec/router drift test) after the YAML regen; `cd lib/packages/fabro-api-client && bun run generate && bun run typecheck`; `cd apps/fabro-web && bun test && bun run typecheck`.
5. `cargo dev docs check` (runs in CI).
6. Manual cluster smoke — **no repo tooling covers a real cluster**, so: `kind create cluster`, `kubectl create ns fabro-test`, apply an RBAC role for pods/exec/networkpolicies, then `cargo nextest run -p fabro-sandbox --profile e2e --run-ignored only -E 'test(kubernetes)'`; end-to-end: start the server with an environment `provider = "kubernetes"` and run a small workflow, then confirm pod + policy are gone afterward and `GET /api/v1/sandboxes` lists it mid-run.

## Deliberately not doing

- **No embedded terminal session** (`terminal.rs` returns "not supported"), **no install-wizard/API install integration**, **no preview URLs/VNC** — the human chose "Core only."
- **No dockerfile support** — rejected at config validation; in-cluster builds (Kaniko/BuildKit) are out of scope.
- **No fabro-on-Kubernetes deployment packaging** (Helm/manifests) — Q1 answer ruled it out.
- **No new settings/env vars/secrets** beyond `providers.kubernetes.enabled` — connection and namespace come from kube-standard inference; pull secrets work via ServiceAccount-level `imagePullSecrets`. Per-environment namespace/cluster and explicit overrides are additive future work.
- **No per-run ServiceAccounts/securityContext knobs, no autostop** (`set_autostop_interval` no-ops, matching Docker), **no metrics, no demo-mode card, no standalone kube twin** (twins are reserved for LLM providers by the testing strategy).
- **No changes to Docker/Daytona/Local behavior** — including leaving `managed_labels::for_run` untouched so Docker label output is byte-identical.