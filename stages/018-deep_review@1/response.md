Review complete. Here is my assessment as the final gate.

## What I verified

**Goal conformance.** The change delivers exactly what was asked and answered: a fourth `SandboxProviderKind::Kubernetes` running one pod per run (lowercased RFC1123 name from the run id, unit-tested), a complete `Sandbox` implementation (exec, streaming exec with stdin, stdio processes, tar-over-exec file ops, clone honoring the documented exact-SHA/no-fallback contract via the shared `clone_source` builders, git push over the shared credential machinery), the inventory provider with managed-label guards and 404-idempotence, per-pod NetworkPolicies (block = deny-all, CIDR = ipBlock + DNS egress, ownerRef GC), `stop()` = pod deletion, kube-standard connection inference with no new secrets, and the Q2 exclusions honored everywhere (no terminal, no install wizard — which the install policy writer explicitly keeps disabled, dockerfile rejected at config validation *and* hidden in the web form).

**All six corrections from my plan critique are present and correct**: web environments surfaces including the automations gating fix; bare-pod-name identity consistent across `sandbox_info`/`runtime.id`/reconnect/details; RFC1123 lowercasing; the `clone_source` and all cfg gates; the no-sentinel stdio path (dedicated wrapper, stream-close termination, cancel wired); kube feature flags including `ws`.

**Would the tests fail if the code were wrong?** For everything testable here, yes — the sentinel reader has adversarial cases (binary payloads without trailing newlines, negative exit codes, marker-text collisions), policies/manifests/naming/labels all have assertions, httpmock covers list/get/delete label semantics, and config validation plus the new wiring arms are tested. The websocket transport is live-gated (`tests/kubernetes_live.rs`, well-documented kind recipe) — it could not run here (no docker/kind/kubectl in this environment), which matches how the Docker provider ships.

**Runs**: workspace build ✓; clippy and fmt clean on all touched crates; fabro-sandbox/types/config/environment/install 942/942 ✓; fabro-workflow 1444/1444 ✓; fabro-server 902/904 with **both failures reproduced on main** (pre-existing `sqlite_blob_activation` environment failures); web typecheck ✓, test failures pre-exist on main (27 there vs 26 here — this change's new tests pass).

## Non-blocking observations for reviewers

1. Non-streaming `exec_command` timeout/cancel discards partial output; the streaming path (which runs agent commands) captures it correctly.
2. Mid-stream transport errors surface as exit −1/`Exited` rather than a hard error — edge case, diagnostics-only.
3. The clone block is a faithful re-implementation (~340 lines) rather than an extraction — explicitly accepted at the refactor-check stage; every shared builder is reused.
4. `k8s-openapi` uses `features = ["latest"]` rather than a pinned cluster version — works, but a pin would be friendlier for releases.
5. First contact with a real cluster is unverified in CI — the PR description should say so and point at the live-test recipe.

The change is faithful to the approved plan, honors both human answers, verifies as far as this environment permits, and its residual risk is disclosed and gated behind a documented test. It is worth reviewer time.

{"preferred_next_label": "approve"}