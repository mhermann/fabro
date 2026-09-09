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

## Context
- parallel.branch_count: 4
- parallel.results (16.6 KB; full value: `/tmp/fabro/runtime/blobs/266c452279f703817712faf887e451eee4ed05f986e06db3bc5f673d190aac7d.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. Here is my correctness review of the SearXNG search backend change.\n\n## Verdict\n\n**The Rust code is essentially correct — I found one minor edge-case divergence and no logic defects","last_stag…


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
