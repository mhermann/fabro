Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?

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
  - Files: /workspace/fabro/.env.example, /workspace/fabro/docs/public/administration/server-configuration.mdx, /workspace/fabro/docs/public/administration/troubleshooting.mdx, /workspace/fabro/docs/public/agents/prompts.mdx, /workspace/fabro/docs/public/agents/tools.mdx, /workspace/fabro/docs/public/changelog/2026-09-10.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/searxng.mdx, /workspace/fabro/lib/apps/fabro-server/src/demo/mod.rs, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/components/fabro-agent/src/cli.rs, /workspace/fabro/lib/components/fabro-agent/src/config.rs, /workspace/fabro/lib/components/fabro-agent/src/web_search.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3

## Context
- parallel.branch_count: 4
- parallel.results (17.5 KB; full value: `/tmp/fabro/runtime/blobs/f54d829de1b84ca202dbd09b412d30ecdd62a824dc243628acbb821595001041.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I examined every hunk of the 15-file diff, read the full `web_search.rs`, `diagnostics.rs` context, all `ToolSecrets` consumers, registration sites, and the legacy secrets migration, ","last_stage"…


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
