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
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/docs/public/administration/deploy-railway.mdx, /workspace/fabro/docs/public/administration/security.mdx, /workspace/fabro/docs/public/administration/server-configuration.mdx, /workspace/fabro/docs/public/integrations/searxng-search.mdx
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3
- **implement**: succeeded
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/docs/public/administration/deploy-railway.mdx, /workspace/fabro/docs/public/administration/security.mdx, /workspace/fabro/docs/public/administration/server-configuration.mdx, /workspace/fabro/docs/public/integrations/searxng-search.mdx
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3

## Context
- parallel.branch_count: 4
- parallel.results (16.6 KB; full value: `/tmp/fabro/runtime/blobs/266c452279f703817712faf887e451eee4ed05f986e06db3bc5f673d190aac7d.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. Here is my correctness review of the SearXNG search backend change.\n\n## Verdict\n\n**The Rust code is essentially correct — I found one minor edge-case divergence and no logic defects","last_stag…


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
