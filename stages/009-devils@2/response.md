I attacked the revised plan, focusing on whether the new Step 3 (run admission) is actually correct against the code, and hunting for new blocking defects. This is my second critique, so I applied a raised bar: only work that would be redone or expensive-to-reverse decisions count.

## What I checked and found sound

**Step 3's anchors are real.** `GitRunTarget` (run_intent.rs:69, `deny_unknown_fields`) taking an optional `instance_url` follows the existing `tag`/`sha` optional-field pattern exactly; `validate()` (line 83) is the single validation chokepoint and its only `.repository()` consumer is `automation_materializer.rs`, which the plan guards. `observe_git_run_target` has exactly one production caller (`run/create.rs:161`) — threading is contained as claimed. The automation guard is the right mechanism to keep the out-of-scope exclusion without narrowing local runs.

**Previously-raised items are genuinely resolved, not papered over:** `normalize_repo_origin_url`/`ssh_url_to_https` verified host-generic (a real read, and the forge path inherits correct normalization for free); the draft-prefix doc wording now states `WORK_IN_PROGRESS_PREFIXES` is instance-configurable; the twin git endpoint is explicitly deferred rather than silently load-bearing; the verification plan now includes admission-level tests (manifest, CLI, workflow twin tests) so the "wrong but compiling" failure mode from round one fails a test.

## Near-misses I considered and rejected as blocking

1. **Server-side worker credential assembly (`worker_runtime.rs`).** If the server builds `StartServices` for any run path besides the CLI worker, forgejo credentials need mirroring there too. But the `maybe_build_github_credentials` pattern establishes exactly where credential assembly lives; the implementer follows the existing call sites, and a missed one surfaces as the plan's own soft-skip warning in logs, not silent wrongness. Ordinary judgment call.
2. **Gitea `state=open` filter support.** The twin will match whatever the client sends, so twin-based tests can't prove the real API accepts the param. But the plan already carries the mitigations that matter: env-gated `live_access.rs` plus the manual dual-instance smoke. Worst case the implementer flips to client-side filtering — cheap, not rework.
3. **`GitRunTarget.instance_url` accepted via public `RunIntent` API.** The scope exclusion (server-submitted forge targets) was about automations and playground UX; `validate()` accepting a valid forge target submitted by an API client just runs it through the same credential-gated path as a CLI run — harmless, arguably correct. The automation guard covers the exclusion that mattered.
4. **`deny_unknown_fields` forward-compat (old binary reading new data).** Same trade the repo already made when `tag`/`sha` were added; downgrades aren't a supported promise here.
5. **Slug grammar limits (39/100 chars) applied to forge owners.** Documented in the plan and docs step; edge-of-edge case, reversible later with a forge-specific slug type if ever needed.

## Verdict

The one blocking defect from round one is fixed with verified anchors and tests that would catch its absence. What remains is ordinary implementation risk — the kind later review stages and the plan's own verification steps (workspace tests, OpenAPI conformance, live smoke against both a real Forgejo and a real Gitea) are designed to catch. I have no remaining objection that would force work to be redone.

{"preferred_next_label": "approve"}