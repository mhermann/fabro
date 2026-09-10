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
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3

## Context
- parallel.branch_count: 4
- parallel.results (25.3 KB; full value: `/tmp/fabro/runtime/blobs/98f8c0d36a54facc356e82304644abfd530847bd1a6cb93b4e5277e4f93c7b29.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I read the full new provider (`kubernetes.rs` ~3,000 lines, `provider/kubernetes.rs`, `tests/kubernetes_live.rs`), every modified file in the diff, and cross-checked the new transport","last_stage"…


Write the pull request description for the change you just made.

The pull request may already exist — this stage also runs again after review
comments have been addressed. Write the description for the change as it stands
now, in full. The next stage creates the pull request if there is none and
updates the existing one if there is. Do not write a changelog of what you fixed
since the last version; write the description the reviewer should read today.

Write exactly two files:

1. `/tmp/pr-title.txt` — a single line. Imperative mood, no trailing period, no
   more than 70 characters. Describe what the change does, not that it is a
   change.
2. `/tmp/pr-body.md` — the description. Cover what changed and why, anything a
   reviewer should look at closely, and how it was verified. If review findings
   were fixed along the way, say what they were.

Do not write these files anywhere inside the repository — only under `/tmp`.

Do not use the shell to create or update the PR. A later stage does that. Never
merge a pull request, and never run `gh pr merge` — this workflow does not merge.
