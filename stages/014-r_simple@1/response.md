I have everything I need. Here is the complexity-only review.

# Review: Kubernetes sandbox provider — unnecessary complexity

## 1. The git clone pipeline is a ~380-line verbatim copy of `docker.rs`'s (most significant)

`kubernetes.rs` duplicates Docker's entire clone orchestration, unchanged except for the exec method name, `Docker`→`Kubernetes` in strings, and the failure-struct name:

| Duplicated item | docker.rs | kubernetes.rs |
|---|---|---|
| `git_clone_command` | 1588 | 1552 |
| `verify_managed_labels` | 1645 | 1593 |
| `CloneStep` enum | 71 | 1403 |
| `DockerCloneFailure`/`KubernetesCloneFailure` | 76 | 1409 |
| `clone_failure_error` | 787 | 1008 |
| `report_clone_failure` | 805 | 1028 |
| `run_exact_local_git_command` | 822 | 1043 |
| `retry_git_transfer` | 849 | 1070 |
| `clone_github_repo` | 909 | 1130 |

Every deadline computation, retry-plan construction, credential-context mapping, pinned-checkout step, and error message now exists twice and must be kept in sync by hand — e.g. a fix to the deadline-expiry path or the token-minting comment block has to be applied in both files.

The seam already exists and the new code already uses it: `sandbox.rs` holds provider-neutral logic as free functions over `&dyn Sandbox` (`setup_git_via_exec`, `git_push_via_exec`, called from kubernetes.rs at lines 2527 and 2553). The clone block is the same category — `exec_command`/`exec_command_streaming` on the trait are sufficient; the only per-provider inputs are the provider kind and the label prefix, both expressible as parameters. Simpler version: move `retry_git_transfer`, `run_exact_local_git_command`, `clone_failure_error`, `report_clone_failure`, `clone_github_repo`, `git_clone_command`, and `verify_managed_labels` into `sandbox.rs` (or `clone_source.rs`) as free functions taking `&dyn Sandbox` + `SandboxProviderKind` + label prefix; docker.rs and kubernetes.rs each drop ~380 lines and keep only their exec transport. The pipeline's own `refactor_check` stage anticipated exactly this extraction "during implementation, guided by the real second consumer" — the second consumer now exists, and it was copied instead.

## 2. Two parallel exec read-loop implementations inside `kubernetes.rs`

`KubernetesSandbox::kubernetes_exec` (lines 631–698) and the free function `run_streaming_exec` (lines 1440–1550) implement the identical biased-select dual-stream read loop, post-loop drain, and `ExecSentinelReader` finish — ~75 lines each, structurally the same, already cosmetically diverged in the drain loops. Since `OutputCaptureBuffer::new(None)` is unbounded and byte-exact, the buffered variant is fully subsumed: `kubernetes_exec_shell` and `download_file_bytes` (binary tar included) can call `run_streaming_exec` with `stdin: None`, `output_callback: None`, `cap: None`. Delete `kubernetes_exec` and `pod_exec_raw`'s buffered role (~70 lines). Docker's two variants genuinely differ (sentinel-free loop vs `inspect_exec`); Kubernetes' two do not — both must strip a sentinel, so there is only one loop worth having.

## 3. The kill-wrapper shell script exists twice

`controlled_shell_command` (303–350) and `controlled_shell_command_no_marker` (2625–2668) are the same ~45-line script except for the `cd`-failure arm and the trailing `echo "{marker}=$status"`. The watcher loop, pid-file handshake, and pgid TERM/KILL escalation are duplicated verbatim — a future fix to the kill logic applied to one copy silently leaves stdio (MCP) sessions inconsistent with exec sessions. Simpler version: one builder with `marker: Option<&str>` that appends the sentinel echo and the `cd`-failure echo conditionally.

## 4. Dead code: `Error::KubernetesConnect` and `Error::kubernetes_connect()`

`error.rs:49-54,124-127` add the variant and constructor; grep confirms zero callers — `connect()` builds the error with `Error::context(...)` instead. Delete both (the `#[cfg(feature = "kubernetes")]` gate hid it from non-k8s builds).

## 5. Dead arm: `network_policy_manifest` with `AllowAll`

Production only builds a policy when `!matches!(network, AllowAll)` (line 556), so the `AllowAll => None` arm (1716) is reachable only from its own unit test `network_policy_allow_all_needs_no_manifest`, which asserts properties of a manifest nothing ever creates. Delete the arm and the test, or gate the test the same way the code is gated.

## Minor

- **`check_kubernetes_sandbox_with_probe` (diagnostics.rs:675)** is a generic probe-injection seam with no test callers — Docker's twin has three tests using it, and the cloud check inlines without a seam. Either add the tests that justify it or inline it.
- **"Pod already exists" is handled twice**: `initialize()` pre-checks `get_pod` (1944–1956) and `create_pod` separately maps HTTP 409 (541–545). For run-scoped pods the pre-check always wins; for preflight pods the UUID name makes 409 unreachable. One mechanism suffices.
- **Single-caller wrappers**: `pod_identifier()` (486) is a one-line pass-through to `pod_name()` with one caller (provider create); `classify_kubernetes_clone_result` (1573) is a one-line wrapper with one caller. Inline both. Likewise `KubernetesSandbox::new` validates `decide_clone` only when tag/sha are present (371–379) — a partial duplicate of initialize's unconditional validation; either validate always or drop it.

## Appropriately simple (no action)

The enum/cfg-gate widenings, config plumbing (`from_environment` trio, `resolve_*` one-liners, dockerfile rejection), server registry/diagnostics/run-manifest arms, web changes (picker, Dockerfile-source suppression, provider panel), OpenAPI/TS regen, docs/changelog, httpmock provider tests, and the ignored live tests all mirror the established Docker/Daytona extension pattern with no speculative abstraction. The `ExecSentinelReader` itself is the minimal correct design for a transport without exit codes.

**Net:** findings 1–3 are the real cost (~520 duplicated lines that will drift); 4–5 are free deletions; the minors are small. Everything else lands cleanly.