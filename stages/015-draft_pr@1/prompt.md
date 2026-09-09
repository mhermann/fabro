Goal: Add support for forgejo similar to github integration

## Completed stages
- **understand**: succeeded
  - Model: glm-5.3
- **ask**: succeeded
  - Model: glm-5.3
- **triage**: succeeded
  - Model: glm-5.3
- **plan**: succeeded
  - Model: glm-5.3
- **devils**: succeeded
  - Model: glm-5.3
- **implement**: succeeded
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/apps/fabro-web/app/routes/settings-integrations.test.tsx, /workspace/fabro/apps/fabro-web/app/routes/settings-integrations.tsx, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/forgejo.mdx, /workspace/fabro/lib/apps/fabro-cli/Cargo.toml, /workspace/fabro/lib/apps/fabro-cli/src/args.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/repo/init.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/forgejo.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/install.rs, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/system.rs, /workspace/fabro/lib/apps/fabro-server/src/server/pull_request_supervisor.rs, /workspace/fabro/lib/components/fabro-forgejo/Cargo.toml, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests_mock.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-manifest/Cargo.toml, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/daytona/mod.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/docker_streaming.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/git_bridge.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/fork.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/mod.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/mod.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/publish.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/types.rs, /workspace/fabro/lib/components/fabro-workflow/src/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_options.rs, /workspace/fabro/lib/foundation/fabro-api/build.rs, /workspace/fabro/lib/foundation/fabro-api/tests/pull_request_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/system_integrations_round_trip.rs, /workspace/fabro/lib/foundation/fabro-client/src/client.rs, /workspace/fabro/lib/foundation/fabro-config/Cargo.toml, /workspace/fabro/lib/foundation/fabro-config/src/layers/mod.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/lib.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/run.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_run.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-types/src/lib.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/repository.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/src/system_integrations.rs
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3

## Context
- parallel.branch_count: 4
- parallel.results (28.4 KB; full value: `/tmp/fabro/runtime/blobs/34a5075081c4430325e2d4ac75a7e5bf5d1e4ac57fc5265536f7cd07ebd4c6cd.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"# Review: Forgejo integration (main...HEAD, ~4,600 lines)\n\nI read the full diff plus surrounding context in the sandbox, workflow, server, and CLI layers. The overall structure is sound: provider sele","last_stag…


Write the pull request description for the change you just made.

Write exactly two files:

1. `/tmp/pr-title.txt` — a single line. Imperative mood, no trailing period, no
   more than 70 characters. Describe what the change does, not that it is a
   change.
2. `/tmp/pr-body.md` — the description. Cover what changed and why, anything a
   reviewer should look at closely, and how it was verified. If review findings
   were fixed along the way, say what they were.

Do not write these files anywhere inside the repository — only under `/tmp`.

Do not use the shell to create the PR. A later stage does that.
