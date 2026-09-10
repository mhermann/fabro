All critique points verified against the code — `StdioProcessTermination::exited(Option<i32>)` makes `exit_code: None` legal, `automation-form.tsx` filters environments through `isCloneBasedEnvironment` at two sites, and the handler's Docker access arm is a clean reconnect + `ssh_access_command()` shape to mirror. Here is the revised plan.

---

# Implementation plan (rev 2): `kubernetes` sandbox provider (core tier)

**What changed from the critique — all points accepted, none rejected:**
1. **Added the missing web environments surfaces** (Step 8): `environment-providers.ts` (`CREATABLE_PROVIDERS`, `isCloneBasedEnvironment`), `environment-form.tsx` (provider parsing + hide Dockerfile for kubernetes). This was a real miss: without it the provider is un-creatable from the primary UI and web-side automations gating contradicts the server's `is_clone_based()`.
2. **Canonicalized the sandbox identity**: `sandbox_info()` and `runtime.id` are the **bare pod name** (namespace re-derived from config inference at reconnect) — the previous draft's `kubernetes:<ns>/<pod>` conflicted with `reconnect`/`details`/registry-`get` treating `runtime.id` as a pod name.
3. **Pod names are RFC 1123**: `fabro-<run_id.to_lowercase()>` (ULIDs are uppercase base32; the API server rejects mixed-case names). Label *values* keep the original ULID.
4. **Added the missed `clone_source` cfg gate** to the file list.
5. **`spawn_stdio_process` does not use the `__FABRO_RC__` sentinel** — MCP stdio is length-prefixed JSON; termination is detected via attach-stream close and reported as `StdioProcessTermination::exited(None)` (legal per the type).
6. **Specified kube feature flags**: `kube = { features = ["client", "config", "rustls-tls", "ws"] }` — `ws` is required for exec/attach; `rustls-tls` matches the workspace's rustls-only stance.

Authoritative inputs unchanged: Q1 = **fourth sandbox provider** (Pod-per-run, no fabro-on-k8s packaging); Q2 = **core only** (no terminal, no install wizard, no previews, dockerfile rejected). Sweep decisions carry over (kube-rs, exec-based cloning, NetworkPolicy for `block`/`cidr`, stop = delete pod, kube-standard inference, no new settings keys beyond `enabled`, provider string `kubernetes`).

## Design specifics

- **Pod-per-run**: name `fabro-<lowercased run_id>` (or `fabro-<short-uuid>` for preflight), labels `sh.fabro.managed=true`, `sh.fabro.run_id=<original ULID>` (when present), and unique `sh.fabro.sandbox=<pod-name>` (the NetworkPolicy selector). `command: ["sleep","infinity"]`, `restartPolicy: Never`, resources requests=limits (cpu cores, memory bytes, `resources.disk` → `ephemeral-storage` quantity), default image `buildpack-deps:noble` (satisfies the `/bin/bash` contract).
- **Exec transport**: kube exec/attach WebSocket streams. The k8s exec API has no per-exec env or working-dir parameters, so commands are wrapped: `cd <cwd> && env K=V … BASH_ENV= /bin/bash -c '<shell_quote(cmd)>'` (Daytona's approach). It also returns no exit code, so the **exec paths** (not stdio) append a trailing `__FABRO_RC__=<n>` stdout marker that the reader parses and strips before delivering bytes to callers/callbacks (the reader holds back only the final unterminated line). Timeout/cancel = client-side deadline + follow-up kill exec (wrapper runs the shell under `setsid` so the kill targets the process group), reported as `TimedOut`/`Cancelled`.
- **`spawn_stdio_process`**: attach with stdin; bidirectional via the stdin channel; exit detected by stdout/stderr channel close → `StdioProcessTermination::exited(None)`. No sentinel.
- **File transfer**: no pod copy API — upload = exec `tar -x` fed via stdin; download = exec `tar -c` read from stdout (`kubectl cp`'s technique). read/write/glob/grep/walk reuse the shared remote command builders already cfg-gated for docker/daytona.
- **Network**: `allow_all` = no policy; `block` = per-pod NetworkPolicy `policyTypes:[Egress], egress:[]`; `cidr_allow_list` = `ipBlock` entries + kube-dns egress (UDP/TCP 53) so names resolve. Policies carry an `ownerReferences` to the Pod for API-server GC.
- **Clone/push**: identical to Docker — `initialize` clones via exec using `clone_source`/`git_retry`; `setup_git`/`git_push_ref`/`refresh_push_credentials` ride the shared `push_credentials` machinery.
- **Connection**: `kube::Config::infer()` (in-cluster SA → `KUBECONFIG` → `~/.kube/config`); namespace = inferred default namespace; no vault secret, no new env vars. Registry registration stays sync via a lazily-built `OnceCell<kube::Client>`; unresolvable config surfaces as a provider error in the fail-soft inventory `meta`.
- **Identity**: `sandbox_info()` = bare pod name → persisted as `runtime.id`; `reconnect`, `details`, and registry `get(id)` all treat it as the pod name; namespace comes from inference at each use.

## Files, changes, and order

Variant addition breaks every exhaustive `match` downstream, so: deps → variant + mechanical arms + stub provider (compiles, k8s paths return "not implemented") → real internals → wiring → contract/UI/docs → tests.

### Step 1 — Dependencies
1. **`Cargo.toml`** (root): add `kube` (features `client`, `config`, `rustls-tls`, `ws`) and `k8s-openapi` (pinned cluster-version feature, latest stable) to `[workspace.dependencies]`. `Cargo.lock` updates are part of the change (CI builds `--locked`).
2. **`lib/components/fabro-sandbox/Cargo.toml`**: feature `kubernetes = ["dep:kube", "dep:k8s-openapi"]`.

### Step 2 — Foundation types (`lib/foundation/fabro-types`)
3. **`src/sandbox_provider.rs`**: `Kubernetes` variant (serde/strum lowercase); add to `is_clone_based()`; extend parse/display tests.
4. **`src/settings/run.rs`**: `EnvironmentProvider::Kubernetes` + `is_clone_based` + `From` arms.
5. **`src/settings/server.rs`**: `ServerSandboxProvidersSettings.kubernetes` + `for_provider()` arm.
6. **`tests/sandbox_model_serde.rs`**: `"kubernetes"` parse/serialize assertion.

### Step 3 — Config layer (`lib/foundation/fabro-config`)
7. **`src/layers/server.rs`**: optional `kubernetes` provider layer entry.
8. **`src/resolve/server.rs`**: resolve it (default `enabled = true`).
9. **`src/resolve/environment.rs`** — `validate_provider_capabilities`: `Kubernetes` arm rejecting `image.dockerfile` (Inline or Path) with a clear reason; all three network modes accepted.
10. **`src/tests/resolve_run.rs` / `resolve_server.rs`**: dockerfile-rejection and `providers.kubernetes.enabled = false` tests.

### Step 4 — Sandbox crate skeleton (workspace compiles again)
11. **`src/sandbox.rs`**: widen cfg gates to `any(feature = "docker", feature = "daytona", feature = "kubernetes")` on `REMOTE_BASH`, `REMOTE_WALK_TIMEOUT_MS` (~lines 34/38) and `resolve_path`, `join_sandbox_path`, `build_remote_walk_command`, `parse_remote_walk_output` (~1554–1611).
12. **`src/lib.rs`**: widen the `clone_source` module gate (~line 9) and `from_environment` gate; add `#[cfg(feature = "kubernetes")] pub mod kubernetes;`; re-exports.
13. **`src/managed_labels.rs`**: widen `is_managed` cfg to include `kubernetes`. Leave `for_run`/`merge_for_run` untouched (Docker output stays byte-identical); kubernetes composes its own label map from the exported constants plus its `sh.fabro.sandbox` key.
14. **NEW `src/kubernetes.rs`** (stub first): `KubernetesSandboxOptions` (`image` default `buildpack-deps:noble`, `env_vars`, `memory_limit`, `cpu`, `ephemeral_storage_limit`, `network: KubernetesNetworkMode`, `clone_depth`, `skip_clone`); `KubernetesSandbox` mirroring `DockerSandbox` internals (`OnceCell` pod name/working dir/origin, `PushCredentialState`, run/clone fields, event callback), constructor validation via `clone_source::decide_clone`.
15. **`src/sandbox_spec.rs`**: `SandboxSpec::Kubernetes` variant; `provider_name()`; `build()`; `to_run_sandbox_instance()` with `workspace_root=/workspace`, `repos_root=/repos` (module-local consts).
16. **`src/provider.rs`**: `SandboxCreateSpec::Kubernetes` variant.
17. **NEW `src/provider/kubernetes.rs`**: `KubernetesSandboxProvider` — `list()` (label selector `sh.fabro.managed`), `get()` (404→`Ok(None)`; managed guard), `create()`, `delete()` (managed-label refusal), lazy `OnceCell` client. `kind()` returns the new variant.
18. **`src/details.rs`**: `Kubernetes` dispatch arm + `mod kubernetes` (phase → `SandboxState`: Pending→Starting, Running→Running, Succeeded/Failed→Stopped/Error; nodeName→region; image; creation timestamp).
19. **`src/reconnect.rs`**: `Kubernetes` arm — `KubernetesSandbox::reconnect(pod_name, repo_cloned, working_directory, origin, branch, run_id)`; no credential plumbing.
20. **`src/terminal.rs`**: `Kubernetes` arm returning "Kubernetes sandboxes do not support embedded terminals" (mirrors Local).
21. **`src/error.rs`**: kube error mapping alongside `docker_connect`.
22. **`src/from_environment.rs`**: `kubernetes_config_from_environment` + `_with_secrets` (mirror the Docker pair).

### Step 5 — Provider internals (`src/kubernetes.rs`, the bulk)
- Client bootstrap; pod create (labels incl. lowercased name; sleep-infinity command; quantities); poll to Ready with terminal-failure detection (`ErrImagePull`/`ImagePullBackOff`/`CrashLoopBackOff` → init error with remediation, Daytona's poll pattern, ~5m pull allowance).
- Bash probe + `/tmp/fabro/runtime` mkdir in `initialize` (existing contracts).
- Clone flow via exec (decide_clone, shallow fetch, exact-SHA/tag handling — Docker's provider-neutral shell).
- Exec layer: wrapper builder (cwd + `env` + `BASH_ENV=` + `bash -c`), `setsid` process-group, `__FABRO_RC__` sentinel writer/parser with strip-before-delivery, streaming multiplex into `OutputCaptureBuffer`/callbacks, timeout/cancel kill path; `exec_command`, `exec_command_streaming` (incl. `request.stdin` via the stdin channel), `spawn_stdio_process` (stream-close → `StdioProcessTermination::exited(None)`).
- File ops: `read_file(_bytes)`, `write_file`, `delete_file`, `file_exists`, `list_directory`, `grep`, `walk_files` (shared builders), `download`/`upload_file_to_local` (tar-over-exec).
- Lifecycle: `activate`/`start` verify Running (idempotent); `stop()` deletes the pod; `delete`/`cleanup` delete pod (ownerRef GCs the policy), 404-idempotent.
- Git: `setup_git` via `setup_git_via_exec`; `git_push_ref`/`refresh_push_credentials` via `push_credentials`; `resume_setup_commands`.
- Metadata: `working_directory`, `platform`/`os_version` (uname, cached), `sandbox_info()` = bare pod name, `runtime_directory`, `ssh_access_command` = `kubectl exec -it <pod> -n <ns> -- sh -lc 'cd <wd> && exec sh -l'`, `origin_url`.

### Step 6 — Workflow + server wiring
23. **`lib/components/fabro-workflow/Cargo.toml`**: add `"kubernetes"` to fabro-sandbox features (line 32).
24. **`src/operations/start.rs`**: `Kubernetes` arm (~line 532) via `kubernetes_config_from_environment_with_secrets(resolved, secret_lookup)`, `skip_clone |= clone_source.skip_clone`, standard clone fields.
25. **`lib/apps/fabro-server/Cargo.toml`**: add `"kubernetes"` (line 37) — puts it in the shipped binary (`-p fabro-cli` unifies through the server dep).
26. **`src/server.rs`** — `build_sandbox_provider_registry()` (~2344): register when `provider_settings.kubernetes.enabled`.
27. **`src/run_manifest.rs`**: `preflight_sandbox_spec` Kubernetes arm (skip_clone=true, ~925); `clone_disabled_for_provider` gains `Kubernetes` (~690); `environment_capability_warnings` Kubernetes arm (~714): warn "kubernetes provider ignores cwd" and "ignores lifecycle.auto_stop"; labels and disk are honored (no warning).
28. **`src/server/handler/runs.rs`** — `validate_intent_environment` (~1104): `image_incompatible` adds `Kubernetes => false` (config layer enforces dockerfile rejection); update the two "Docker or Daytona environment" detail strings to name all three clone-based providers.
29. **`src/server/handler/sandbox.rs`**: access-command match (~420) — `Kubernetes` arm mirroring the **Docker** arm exactly (reconnect via the generic path + `sandbox.ssh_access_command()` → `Ok(Some(command))`); the `== Daytona` preview/VNC guards keep k8s excluded by construction.
30. **`src/diagnostics.rs`**: `check_kubernetes_sandbox` card (policy check → config inference → `client.apiserver_version()` probe → remediation on failure), wired into the `tokio::join!` (~97).
31. **`src/server/tests.rs`**: provider-policy test analog to the Daytona one (~1370) for `kubernetes`.

### Step 7 — API contract
32. **`docs/public/api-reference/fabro-api.yaml`**: `kubernetes` in `SandboxProviderKind` (~12948), `EnvironmentProvider` (~15042), `ServerSandboxProvidersSettings` required+properties (~14456). `InstallSandboxInput`/`Summary` stay `docker|daytona` (~6590) per Q2.
33. Regenerate: `cargo build -p fabro-api` (progenitor; `with_replacement` already maps `SandboxProviderKind` — no build.rs change).
34. **`lib/packages/fabro-api-client`**: `bun run generate` + typecheck. Required for the web steps below (the new `providers.kubernetes` key must exist in the TS types first).

### Step 8 — Web (revised)
35. **`apps/fabro-web/app/lib/environment-providers.ts`**: add `EnvironmentProvider.KUBERNETES` to `CREATABLE_PROVIDERS`. This single change fixes both the "New environment" picker (`settings-environments.tsx` filters this list against `providers[provider].enabled`, picking kubernetes up automatically once the TS types include it) **and** the automations gating — `automation-form.tsx` filters environments through `isCloneBasedEnvironment` at two sites (126, 301), so kubernetes environments become selectable for Git-targeted automations with no edit to that file.
36. **`apps/fabro-web/app/components/environment-form.tsx`**: `parseCreatableProvider` gains a `KUBERNETES` case (currently defaults anything unexpected to Docker); kubernetes branch hides/disables the Dockerfile `ImageSource` option (image reference only, matching the server-side validation) and shows all three network modes.
37. **`apps/fabro-web/app/routes/settings-sandboxes.tsx`**: `"kubernetes"` in `SandboxProviderId` (~20) + provider card (~55–64).
38. Extend the web tests that cover these components (`automation-form.test.tsx`, and the environment-form/settings-environments tests if present) with a kubernetes case; run `bun test` + `bun run typecheck`.

### Step 9 — Docs
39. **`docs/public/administration/sandboxing.mdx`**: four-provider sentence; Kubernetes subsection — connection inference (in-cluster SA / kubeconfig), required RBAC (pods + exec + networkpolicies in one namespace), CNI-enforcement caveat for network modes, `stop` = pod deletion.
40. **`docs/public/execution/environments.mdx`**: Kubernetes provider section — image default/required, dockerfile rejected, resources mapping (disk → ephemeral-storage), labels → pod labels, network modes, clone-based contract, no embedded terminal.
41. **NEW `docs/public/changelog/2026-09-10.mdx`**: dated entry in the existing format; notes the provider and the downgrade caveat (records with `provider: "kubernetes"` won't deserialize on older binaries — same property Daytona's addition had).

### Step 10 — Tests (written with their steps)
- Unit (`fabro-sandbox`): label-map composition (incl. reserved-label override), pod-name generation (lowercasing, preflight names), env-wrapper builder quoting, `__FABRO_RC__` sentinel parser (success/timeout/cancel/marker-collision cases), NetworkPolicy construction for all three modes, pod-spec quantities, phase→state mapping.
- httpmock-backed REST tests: `kube::Config` pointed at the mock server (plain HTTP, no auth) covering list/get/delete label filtering, managed-label refusal, 404 idempotence; fallback if the custom-config path fights us: `kube::Client` from a mock tower service (kube's documented testing pattern).
- Live-gated **NEW `lib/components/fabro-sandbox/tests/kubernetes_live.rs`** (mirrors `docker_streaming.rs`): `#[ignore = "requires a Kubernetes cluster…"]`, env-gated skip — initialize+probe (with a run_id, proving the lowercased name), exec exit codes, streaming, tar round-trip, stop deletes pod+policy, reconnect round-trip (proves the bare-pod-name identity end to end).
- `fabro-types`/`fabro-config` tests per Steps 2–3.

## Verification

1. `cargo build --workspace` after every step (stub-first ordering keeps it green).
2. `cargo nextest run -p fabro-types -p fabro-config -p fabro-sandbox -p fabro-workflow -p fabro-server`, then `cargo nextest run --workspace`.
3. `cargo +nightly-2026-04-14 fmt --all`; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
4. API conformance: `cargo nextest run -p fabro-server`; `cd lib/packages/fabro-api-client && bun run generate && bun run typecheck`; `cd apps/fabro-web && bun test && bun run typecheck`.
5. `cargo dev docs check`.
6. Manual cluster smoke — **no repo tooling covers a real cluster**: `kind create cluster`, `kubectl create ns fabro-test`, apply RBAC (pods/exec/networkpolicies), run `cargo nextest run -p fabro-sandbox --profile e2e --run-ignored only -E 'test(kubernetes)'`; then end-to-end: create a kubernetes environment **through the web UI** (proves Step 8), run a small workflow against it, confirm pod + policy are gone afterward and `GET /api/v1/sandboxes` lists it mid-run.

## Deliberately not doing

- **No embedded terminal**, **no install-wizard/API install integration**, **no preview URLs/VNC** — Q2 "Core only."
- **No dockerfile support** — rejected at config validation and hidden in the web form; in-cluster builds out of scope.
- **No fabro-on-Kubernetes deployment packaging** — Q1 ruled it out.
- **No new settings/env vars/secrets** beyond `providers.kubernetes.enabled` — connection and namespace from kube-standard inference; pull secrets via ServiceAccount-level `imagePullSecrets`.
- **No per-run ServiceAccounts/securityContext knobs, no autostop** (no-ops, matching Docker), **no metrics, no demo-mode card, no standalone kube twin** (reserved for LLM providers by the testing strategy).
- **No changes to Docker/Daytona/Local behavior** — `managed_labels::for_run` untouched so Docker label output stays byte-identical; `automation-form.tsx` and `settings-environments.tsx` need no edits (they consume `CREATABLE_PROVIDERS`).