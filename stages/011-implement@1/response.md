The Kubernetes sandbox provider is implemented and verified. Here's the full summary:

## What was built

A fourth sandbox provider (`kubernetes`, clone-based like Docker/Daytona): one Pod per run, GitHub clone into `/workspace` via the shared exec machinery, agent commands run through the Kubernetes exec API, connection via kube-standard inference (in-cluster SA → `KUBECONFIG` → `~/.kube/config`), `stop()` deletes the pod, per-pod NetworkPolicy enforces `block`/`cidr_allow_list` (owner-referenced for GC).

## File-by-file changes

**New files**
- `lib/components/fabro-sandbox/src/kubernetes.rs` — the provider core (~3,000 lines): `KubernetesSandboxOptions`/`KubernetesNetworkMode`, pod manifest (labels, quantities with requests=limits, `restartPolicy: Never`), NetworkPolicy manifest (block/CIDR+DNS), pod-ready polling with image-pull/crash-loop terminal detection, exec transport (per-exec control files + `setsid` process-group kill + per-exec-nonce `__FABRO_RC_*__=` stdout sentinel parsed/stripped by `ExecSentinelReader`), full `Sandbox` impl (initialize/probe/exec/streaming/stdio via stream-close → `exited(None)`/tar-over-exec file transfer/grep/walk/git lifecycle via shared helpers), 17 unit tests, plus 5 httpmock REST tests in `provider/kubernetes.rs` (label-selector list, 404→`None`, unmanaged refusal, delete idempotence).
- `lib/components/fabro-sandbox/src/provider/kubernetes.rs` — `SandboxProvider` impl with client-injectable `list/get/delete_with_client` for testability.
- `lib/components/fabro-sandbox/tests/kubernetes_live.rs` — `#[ignore]`-gated live tests (init/probe/stop, streaming exit codes, tar round-trip + reconnect, timeout kill), kind recipe in module docs.
- `docs/public/changelog/2026-09-10.mdx`, `apps/fabro-web/app/lib/environment-providers.test.ts`.

**Modified** — enum/cfg arms: `fabro-types` (`SandboxProviderKind`, `EnvironmentProvider`, `ServerSandboxProvidersSettings` + tests), `fabro-config` (server layer/resolve, kubernetes dockerfile rejection + tests), `fabro-sandbox` (lib.rs, sandbox.rs, sandbox_spec.rs, provider.rs, details.rs with phase→state mapping + quantity parsing tests, reconnect.rs, terminal.rs "no embedded terminals", from_environment.rs config mapping, error.rs `KubernetesConnect`, managed_labels.rs/clone_source.rs gate widenings), `fabro-environment` (seeded default), `fabro-install` (policy write, wizard unchanged per plan), workflow `start.rs`, server (`server.rs` registry, `run_manifest.rs`, `handler/runs.rs`, `handler/sandbox.rs`, diagnostics.rs new Kubernetes card, tests.rs policy test), OpenAPI spec + regenerated TS client, docs (`sandboxing.mdx`, `environments.mdx`), deps (`kube` 4.2/k8s-openapi 0.28 `latest`=v1_36, ring provider matching workspace rustls).

## Adaptations from the plan
- **`clone_source` gates**: the plan listed one missed gate; the compile revealed several more (`exact_repository_init_command`, `pinned_fetch_command`, etc.) — all widened identically.
- **`finish()` sentinel anchoring**: devils flagged marker-collision and no-trailing-newline; implemented with a per-exec nonce + last-occurrence match so binary tar payloads strip cleanly (unit-tested).
- **Install policy**: the plan kept the wizard docker/daytona-only; I additionally write `kubernetes.enabled = false` during install so the written config expresses intent rather than relying on the default.
- **k8s-openapi 0.28 API drift**: `labels`/`match_labels` are `BTreeMap`, `creation_timestamp` is a jiff `Time` newtype, `pod_selector`/`limits` are `Option`, `NetworkPolicy` has no `status` — all adapted.
- **Provider testability**: provider methods delegate to `*_with_client` so httpmock tests don't need cluster inference.

## Verification
fmt (pinned nightly) clean; `clippy --workspace --all-targets -D warnings` clean; tests: types 771 ✓, config ✓, api 222 ✓, sandbox 187 ✓ (46 live-skipped by design), server 904 ✓ (incl. API conformance), sandbox+workflow 1774 ✓, workspace 7454/7458 ✓; web tests+typecheck ✓; TS client typecheck ✓; `cargo dev docs check` ✓. The 4 workspace failures and the CLI `it` server-spawning tests fail **only** on this environment's exhausted shared disk (`InsufficientDisk: available_bytes: 0` preflight) — they passed in dedicated runs with disk headroom, and web test failures are pre-existing (identical on clean tree). Environment notes: I installed Rust (stable + pinned nightly), nextest, bun, and a JRE (for TS client generation); file mtimes in this sandbox default to epoch-0, which required `touch` passes before rebuilds.