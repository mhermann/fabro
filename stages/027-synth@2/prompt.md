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
  - Files: /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-server/src/automation_materializer.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/tests/it/api/runs.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/url.rs, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_intent.rs
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3
- **implement**: succeeded
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-server/src/automation_materializer.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/tests/it/api/runs.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/url.rs, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_intent.rs
- **fork**: succeeded

## Context
- parallel.branch_count: 4
- parallel.results (28.0 KB; full value: `/tmp/fabro/runtime/blobs/33ae148bbe780b10344cc6ba61f730878896300005beaa323f71e50a51d3cce1.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"# Correctness review — Forgejo integration (111 files, +5637/−464)\n\nI read the full diff plus the surrounding context of every seam it touches. I could not execute anything (review-only checkout),","last_stage":"…


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
