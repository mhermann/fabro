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
  - Files: /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/test/twin/forgejo/tests/client_ops.rs
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3
- **implement**: succeeded
  - Model: glm-5.3-flash
  - Files: /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/test/twin/forgejo/tests/client_ops.rs
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.3

## Context
- parallel.branch_count: 4
- parallel.results (23.0 KB; full value: `/tmp/fabro/runtime/blobs/57458c95d586d1f5c128c61e21faf79b6a24b391e23c77065c4e4a5b3fdee992.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"I've now read the full new crate, the webhook handler, the clone/preflight dispatch, the pipeline PR host, the wire-format changes, the install/diagnostics/CLI surfaces, and the manifest lane. Here is","last_stage"…


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
