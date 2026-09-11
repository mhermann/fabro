Goal: Add support for forgejo similar to github integration

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
- **refactor_check**: succeeded
  - Model: glm-5.3
- **implement**: succeeded
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/.env.example, /workspace/fabro/AGENTS.md, /workspace/fabro/Cargo.toml, /workspace/fabro/apps/fabro-web/app/components/pull-request-chip.test.tsx, /workspace/fabro/apps/fabro-web/app/install-api.test.ts, /workspace/fabro/apps/fabro-web/app/install-api.ts, /workspace/fabro/apps/fabro-web/app/install-app.test.tsx, /workspace/fabro/apps/fabro-web/app/install-app.tsx, /workspace/fabro/apps/fabro-web/app/install-config.ts, /workspace/fabro/apps/fabro-web/app/install-flow.test.ts, /workspace/fabro/apps/fabro-web/app/install-flow.ts, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/changelog/2026-09-11.mdx, /workspace/fabro/docs/public/core-concepts/forgejo.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/lib/apps/fabro-cli/Cargo.toml, /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/install.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/pr/link.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/run_progress/event.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/run_progress/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/runner.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/runs/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/forgejo.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/mod.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/attach.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/inspect.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_create.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_link.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_view.rs, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/apps/fabro-server/src/forgejo_webhooks.rs, /workspace/fabro/lib/apps/fabro-server/src/install.rs, /workspace/fabro/lib/apps/fabro-server/src/lib.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/serve.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/system.rs, /workspace/fabro/lib/apps/fabro-server/src/server/pull_request_supervisor.rs, /workspace/fabro/lib/apps/fabro-server/src/server/tests.rs, /workspace/fabro/lib/components/fabro-agent/tests/it/docker_shell.rs, /workspace/fabro/lib/components/fabro-forgejo/Cargo.toml, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests_mock.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/daytona/mod.rs, /workspace/fabro/lib/components/fabro-sandbox/src/docker.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider/daytona.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/daytona_streaming_live.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/Cargo.toml, /workspace/fabro/lib/components/fabro-workflow/src/event/convert.rs, /workspace/fabro/lib/components/fabro-workflow/src/event/events.rs, /workspace/fabro/lib/components/fabro-workflow/src/git_bridge.rs, /workspace/fabro/lib/components/fabro-workflow/src/handler/manager_loop.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/start.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/mod.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/publish.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/types.rs, /workspace/fabro/lib/components/fabro-workflow/src/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_options.rs, /workspace/fabro/lib/components/fabro-workflow/src/services.rs, /workspace/fabro/lib/components/fabro-workflow/src/test_support.rs, /workspace/fabro/lib/foundation/fabro-api/tests/pull_request_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/run_integrations_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/run_summary_round_trip.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/run.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/run.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-test/Cargo.toml, /workspace/fabro/lib/foundation/fabro-test/src/lib.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/misc.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/mod.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/run.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/src/system_integrations.rs, /workspace/fabro/test/twin/forgejo/Cargo.toml, /workspace/fabro/test/twin/forgejo/src/auth.rs, /workspace/fabro/test/twin/forgejo/src/handlers/branches.rs, /workspace/fabro/test/twin/forgejo/src/handlers/git.rs, /workspace/fabro/test/twin/forgejo/src/handlers/mod.rs, /workspace/fabro/test/twin/forgejo/src/handlers/pulls.rs, /workspace/fabro/test/twin/forgejo/src/handlers/repos.rs, /workspace/fabro/test/twin/forgejo/src/handlers/users.rs, /workspace/fabro/test/twin/forgejo/src/lib.rs, /workspace/fabro/test/twin/forgejo/src/server.rs, /workspace/fabro/test/twin/forgejo/src/state.rs, /workspace/fabro/test/twin/forgejo/src/test_support.rs, /workspace/fabro/test/twin/forgejo/tests/client_ops.rs
- **fork**: succeeded

## Context
- parallel.branch_count: 4
- parallel.results (23.9 KB; full value: `/tmp/fabro/runtime/blobs/5ea0a594a8da0438cb68f865e5ee350400f8c51cce03e410f58d38dd900dfa55.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. Here is my correctness assessment of the Forgejo integration (127 files, ~8.4k insertions).\n\n## What I verified as correct\n\n- **Webhook HMAC** (`forgejo_webhooks.rs`): constant-time `","last_st…


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
