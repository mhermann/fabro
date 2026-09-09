Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?

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
  - Model: glm-5.3
  - Files: /workspace/fabro/.env.example, /workspace/fabro/docker-compose.local.yaml, /workspace/fabro/docker/searxng/settings.yml, /workspace/fabro/docs/internal/server-secrets-strategy.md, /workspace/fabro/docs/public/administration/server-configuration.mdx, /workspace/fabro/docs/public/agents/tools.mdx, /workspace/fabro/docs/public/changelog/2026-09-09.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/brave-search.mdx, /workspace/fabro/docs/public/integrations/searxng.mdx, /workspace/fabro/docs/public/integrations/venice-search.mdx, /workspace/fabro/lib/apps/fabro-server/src/demo/mod.rs, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/components/fabro-agent/README.md, /workspace/fabro/lib/components/fabro-agent/src/cli.rs, /workspace/fabro/lib/components/fabro-agent/src/config.rs, /workspace/fabro/lib/components/fabro-agent/src/tools.rs, /workspace/fabro/lib/components/fabro-agent/src/web_search.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs
- **fork**: succeeded

## Context
- parallel.branch_count: 4
- parallel.results (18.5 KB; full value: `/tmp/fabro/runtime/blobs/ee0edb760cc2472778020db0409bd7b9e339cf013f19d64e1f40432a25f1cff7.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I read the full diff (`abb07abf..HEAD`, 21 files), traced every consumer of the changed types, and ran the relevant test suites in this checkout — all pass (fabro-agent web_search/c","last_stage":"…


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
