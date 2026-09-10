I have a complete picture now. Here is my review, scoped to test coverage.

---

# Test-coverage review: Kubernetes sandbox provider

## What is genuinely well covered

- **Sentinel exit-code protocol** — 6 unit tests attack exactly the nasty cases (binary payload without trailing newline, marker text colliding with user output, negative codes, malformed sentinel, one-line latency of the hold-back reader). These test behavior (`finish()` returns stripped bytes + decoded code), not implementation.
- **Manifest builders** — pod manifest (quantities, `requests == limits`, `restartPolicy: Never`, label override of user keys, empty-omission) and NetworkPolicy (block = empty egress, CIDR + DNS rule, owner reference) assert the object that actually hits the API.
- **Provider inventory over httpmock** — 5 tests, and they're behavioral: the label selector must be sent, 404 → `None`, unmanaged pods are refused for get *and* delete (asserting the DELETE is never issued), delete is idempotent.
- **Details mapping** — all 5 phase→state branches, CPU millicores/cores, byte SI suffixes.
- **Config/types/web** — dockerfile rejection for kubernetes, provider enable/disable resolution, serde round-trip, `parseCreatableProvider`, `CREATABLE_PROVIDERS` × clone-based gating. The disabled-provider policy error is tested server-side.
- **Live tests** (4, `#[ignore]`) cover init/stop-deletes-pod, streaming exit codes, tar round-trip + reconnect, and timeout-kill verified via a follow-up `pgrep` — the last one is a real behavioral check, not a sleep-and-hope. This matches the `docker_streaming.rs` convention, and CI runs the unit tests via workspace feature unification, same as docker/daytona.

## Gaps — new behavior with no test anywhere

1. **`from_environment::kubernetes_config_from_environment_env` is untested.** The Daytona sibling has `daytona_config_maps_docker_image_to_snapshot`; the kubernetes mapping (image fallback, network-mode mapping, resource conversion, `skip_clone`, env sorting) has nothing. Concrete silent failure: flip `EnvironmentNetworkMode::Block => KubernetesNetworkMode::AllowAll` in that match and **no test in the repo fails** — the config test only resolves `cidr_allow_list`, and the manifest tests take a mode as input rather than producing one. That is a security-relevant mapping passing through untested.

2. **The install-policy adaptation is unasserted.** `write_sandbox_provider_policy` now writes `kubernetes.enabled = false`, but all three existing `write_sandbox_settings_*` tests assert only local/docker/daytona flags — they pass unchanged whether or not the kubernetes line is correct. Adding `assert_eq!(sandbox_provider_enabled(&doc, "kubernetes"), Some(false))` to one of them is a one-line fix for a test suite that currently can't see the behavior.

3. **`create_pod`'s API interaction is untested below the live gate.** The httpmock harness and `with_client` injection exist and are used for list/get/delete — but nothing tests that a pod with `network = block` actually results in a NetworkPolicy **POST**, and `network_policy_allow_all_needs_no_manifest` only asserts `egress.is_none()` on the builder's return value, not that `create_pod` skips creating the object. The test would keep passing if `create_pod` started always applying policies. The 409-already-exists → friendly-error mapping is also untested and pure REST (no exec needed).

4. **`build_single_file_tar` / `extract_single_file_tar` are pure in-memory functions with zero offline tests.** A round-trip (binary bytes, non-file entries skipped, empty-archive error) would run on every CI pass; today the only exercise is the `#[ignore]`d live test, so a header/path regression ships silently until someone runs kind manually.

5. **Runtime-file permission logic is uncovered.** `upload_bytes_to_pod` switches `umask 077`/mode `0o600` for paths under `RUNTIME_DIRECTORY` vs `0o644` elsewhere — a real new behavior (docker has a live test for exactly this: `docker_runtime_directory_is_private_and_outside_workspace`). The kubernetes live file has no analog, and the `is_runtime_path` branch has no unit test.

## Gaps — sibling-parity omissions (the provider next door is tested, this one isn't)

6. **`environment_capability_warnings`**: docker and daytona each have a "cwd is reported as ignored" test; the new kubernetes arm emits two warnings (cwd, `lifecycle.auto_stop`) with none.

7. **`preflight_sandbox_spec`**: Docker has `..._disables_docker_clone_but_preserves_clone_metadata`; the kubernetes arm (`skip_clone = true`, origin/branch preserved) has no analog.

8. **Workflow `start.rs`**: Docker and Daytona each have a "none target forces empty workspace" test destructuring the spec and asserting `config.skip_clone` + empty origin/branch/sha. The new `SandboxSpec::Kubernetes` arm (lines 563–577, including its own `skip_clone |=` line) has no equivalent, and the `kubernetes` feature is enabled in `fabro-workflow`, so the test is feasible without a cluster.

9. **`terminal_failure_detection_names_image_pull_and_crash_reasons` under-delivers on its name**: only `ImagePullBackOff` is asserted. The missing boundary cases are the ones that matter: a pod in `ContainerCreating` (the *normal* pre-Ready state during image pull) must return `None` — a too-broad terminal list would abort every slow pod start, and only the manual live test would notice — and the `phase: Failed` branch is never exercised.

## Minor

- `map_kubernetes_pod` / `kubernetes_fields_from_pod` (creation-timestamp parsing, image fallback from record, region = nodeName) are never directly tested; one httpmock test hits `kubernetes_info_from_pod` with a minimal pod.
- `verify_managed_labels`'s refusal paths (missing managed label, run-id mismatch) are pure functions tested only via the live reconnect happy path.
- `controlled_shell_command_no_marker` (the stdio wrapper) is untested while its marker sibling is. `spawn_stdio_process` has no coverage anywhere — but that matches docker's convention, so it's a disclosed tradeoff rather than a regression.

## Verdict

Coverage of the pure logic core is good and honestly behavioral, but the change has several **untested new behaviors whose failure mode is silent**: the network-mode mapping (finding 1) is the worst — a one-character regression disables egress enforcement with a green CI. Findings 1–5 are the ones I'd insist on before merge; 6–9 are cheap parity fills in files that already have the test scaffolding.