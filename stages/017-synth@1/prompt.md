Goal: Create a provider for running sandboxes in Kubernetes

## Completed stages
- **understand**: succeeded
  - Model: glm-5.3
- **sweep**: succeeded
  - Model: glm-5.3
- **ask**: succeeded
  - Model: glm-5.3
- **triage**: succeeded
  - Model: glm-5.3
- **plan**: succeeded
  - Model: glm-5.3
- **devils**: succeeded
  - Model: glm-5.3
- **plan**: succeeded
  - Model: glm-5.3
- **devils**: succeeded
  - Model: glm-5.3
- **refactor_check**: succeeded
  - Model: glm-5.3
- **implement**: succeeded
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/Cargo.toml, /workspace/fabro/apps/fabro-web/app/components/environment-form.test.ts, /workspace/fabro/apps/fabro-web/app/components/environment-form.tsx, /workspace/fabro/apps/fabro-web/app/components/environment-providers.test.ts, /workspace/fabro/apps/fabro-web/app/lib/environment-providers.test.ts, /workspace/fabro/apps/fabro-web/app/lib/environment-providers.ts, /workspace/fabro/apps/fabro-web/app/routes/settings-sandboxes.tsx, /workspace/fabro/docs/public/administration/sandboxing.mdx, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/changelog/2026-09-10.mdx, /workspace/fabro/docs/public/execution/environments.mdx, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/runs.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/sandbox.rs, /workspace/fabro/lib/apps/fabro-server/src/server/tests.rs, /workspace/fabro/lib/components/fabro-environment/src/store.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/details.rs, /workspace/fabro/lib/components/fabro-sandbox/src/error.rs, /workspace/fabro/lib/components/fabro-sandbox/src/from_environment.rs, /workspace/fabro/lib/components/fabro-sandbox/src/kubernetes.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/managed_labels.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider/kubernetes.rs, /workspace/fabro/lib/components/fabro-sandbox/src/reconnect.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/src/terminal.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/kubernetes_live.rs, /workspace/fabro/lib/components/fabro-workflow/Cargo.toml, /workspace/fabro/lib/components/fabro-workflow/src/operations/start.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/environment.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_run.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-types/src/sandbox_provider.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/run.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/tests/sandbox_model_serde.rs
- **fork**: succeeded

## Context
- parallel.branch_count: 4
- parallel.results (25.3 KB; full value: `/tmp/fabro/runtime/blobs/98f8c0d36a54facc356e82304644abfd530847bd1a6cb93b4e5277e4f93c7b29.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I read the full new provider (`kubernetes.rs` ~3,000 lines, `provider/kubernetes.rs`, `tests/kubernetes_live.rs`), every modified file in the diff, and cross-checked the new transport","last_stage"…


Four reviewers looked at this change from different angles — correctness,
security, tests, and simplicity. Their findings are in the branch results above.

Produce one consolidated report for the deep reviewer who comes next:

1. Merge findings that are the same problem seen from two angles.
2. Discard findings that are plainly wrong, speculative, or mere preference.
3. Order what remains by how much it actually matters.
4. Note explicitly where the reviewers disagreed, or where only one of them saw
   something. Those are the places a second pair of eyes is most valuable, so
   flag them rather than quietly resolving them yourself.

For each surviving finding give the file, what is wrong, and why it matters.

Do not decide whether the change should ship — that is the next stage's job.
Your output is evidence for that decision, not the decision itself. If the
reviewers found nothing worth passing on, say so plainly rather than padding.
