This is the second critique round. The three defects I raised before are each genuinely fixed — I re-verified the fixes against the repository rather than taking the plan's word:

- **Allowlist fix**: `KUBERNETES_SERVICE_HOST`/`PORT` are the actual env gates for kube-rs in-cluster discovery; adding them to `WORKER_ENV_ALLOWLIST` follows the existing AWS-chain precedent in that exact file, and the plan updates the fail-closed test that would otherwise fail. Correct.
- **Token fix**: HMAC over the vault `SESSION_SECRET` + pod name is re-derivable from every process that matters (worker via FABRO_HOME vault access — the mechanism `spawn_env.rs` documents; server via `vault_secret(EnvVars::SESSION_SECRET)` — the call pattern `worker_token_keys_from_server_secrets` already uses). Only the derived token, never the secret, enters pod env. The reconnect flow now has a sound basis.
- **Release fix**: `release.yml`/`nightly.yml` have a docker job pushing `ghcr.io/fabro-sh/fabro` that the agent job can mirror; the default image ref will resolve on release.

New attack surface I probed this round:

1. **`FABRO_KUBERNETES_AGENT_IMAGE` override resolution** — the worker builds specs under `env_clear()`, so an ambient-env-only override would silently not apply to server runs. Non-blocking: the compiled-in default ref works regardless, and the fix is a resolution-order rule (secret store first, then process env — the same pattern the Daytona key uses) the implementer can apply without redesign. Flagging it here so it isn't discovered as a surprise.
2. **Idle SPDY/port-forward longevity** for hour-long stdio processes — operational tuning (keepalive ping), not an architectural flaw; the manual kind checklist plus `#[ignore]`d integration tests cover the real behavior.
3. **`cleanup()` vs `preserve` semantics** — not spelled out per-method, but "exactly the Docker pattern" plus the stated stop/delete/preserve decisions pins it; ordinary judgment call.
4. **UI terminal tab on Kubernetes runs** will surface the explicit unsupported error — the agreed scope, not a regression.
5. Re-checked the exhaustive-match census (run_state, environment store, install, demo, git_retry, run_progress are tests/fixtures/no-ops) — compiler-driven completeness plus the enumerated arms covers it; no hidden coupling found beyond what the plan names.
6. Verification soundness: the restart-mid-run manual check is the test that would catch a wrong token scheme; the allowlist unit test catches a wrong env story; agent protocol tests are process-level, not mock-heavy. Nothing passes vacuously.

The remaining findings are implementer judgment calls that later review stages can catch cheaply. The plan is a sound basis for work.

{"preferred_next_label": "approve"}