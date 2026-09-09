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
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3

## Context
- parallel.branch_count: 4
- parallel.results (18.5 KB; full value: `/tmp/fabro/runtime/blobs/ee0edb760cc2472778020db0409bd7b9e339cf013f19d64e1f40432a25f1cff7.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I read the full diff (`abb07abf..HEAD`, 21 files), traced every consumer of the changed types, and ran the relevant test suites in this checkout — all pass (fabro-agent web_search/c","last_stage":"…


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
