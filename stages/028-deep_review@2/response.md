# Final review

Since my last verdict, a fix round ran (`8cb8d79d..b94c3476`). I re-reviewed the change myself rather than ratifying: I re-read the fixed spots, verified each of my three defect classes against the current tree, and — after freeing disk and installing the pinned toolchain — **ran the repo's actual CI gates**.

## What I verified as resolved

1. **The promised wiring tests now exist and are real.** `tests/it/api/runs.rs` has four forgejo admission/preflight tests (intent rejected without the integration; rejected without a vault token *with remediation*; preflight reports a missing token; preflight verifies repository access against the instance). `automation_materializer.rs` has forgejo PAT-auth clone tests. `fabro-manifest` has the `[run.scm] provider = "forgejo"` guard tests — including the error-without-instance case and github-unchanged. `repo_init.rs` is a new integration test file (forgejo block written, github skipped). And the PR pipeline has `forgejo_origin_creates_a_wip_pull_request_without_touching_github` using httpmock to assert the forgejo client is used (`authorization: token forgejo-pat`) and GitHub is never called. These tests would fail if the wiring were wrong.
2. **Dead API removed.** `FORGEJO_CREDENTIAL_HELPER`, `credential_helper`, `credential_helper_key`, and the `#[expect(dead_code)]` dummy are gone from `fabro-forgejo`.
3. **The two brace-style breaks and the missing trailing newline are fixed.**
4. **Logic re-confirmed:** I re-checked the credential flow (runtime-only `ForgejoContext` on the non-serde `SandboxSpec`, scrubbed record origins), admission gating, worker path via `runner.rs`, PR dispatch, and the nullable SQL migration. All still correct.
5. **Tests I could run:** `cargo nextest run -p fabro-forgejo` → **19 passed, 1 skipped**. (Deeper crates hit environment limits — openssl build + disk — not code failures.)

## The one blocking defect

**The CI formatting gate fails.** I installed the pinned toolchain and ran the exact CI command from CLAUDE.md:

```
cargo +nightly-2026-04-14 fmt --check --all
```

It reports diffs in **49 files** across `lib/apps`, `lib/components`, and `lib/foundation` (e.g. `doctor.rs:25` chain not broken, `preflight.rs` field alignment collapsed, `run_intent.rs`, `fabro-forgejo/src/lib.rs`, `handler/runs.rs`, `automation_materializer.rs`, and the new test files). The fix round hand-patched the two spots I had named by line instead of running the formatter; the rest of the diff was never formatted. A PR opened now reds CI immediately — that is not worth a reviewer's time.

**Fix (mechanical, zero semantic risk):** run `cargo +nightly-2026-04-14 fmt --all`, then confirm `cargo +nightly-2026-04-14 fmt --check --all` exits clean; re-run `cargo nextest run -p fabro-forgejo` (confirmed green pre-format; fmt is whitespace-only) and the other touched crates where the environment allows; run `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings` if the environment permits (I could not — no clippy component installable on this disk; formatting changes don't affect clippy, but the gate should be exercised before opening).

Not escalate: the change matches what the human chose (tier b — CLI + server-side; single instance + provider tag), secrets follow the established runtime-credential pattern, the migration is nullable and backfill-safe, and no decision is open. One mechanical gate stands between this and reviewer-ready.

{"preferred_next_label": "fix"}