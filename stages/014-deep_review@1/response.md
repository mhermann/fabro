Review complete. Here is my independent assessment.

## What I verified myself

**Tests actually run (not trusted from CI claims):**
- `fabro-forgejo` + `fabro-types` + `fabro-config`: 804/804 pass
- `fabro-sandbox` + `fabro-manifest` + `fabro-install`: 385 pass
- `fabro-workflow`: 1,231 lib + 211 integration pass
- `fabro-server`: 903 pass — **including the OpenAPI conformance test** that catches spec/router drift
- `fabro-api` + `fabro-store` + `fabro-static`: 510 pass
- `fabro-cli` `it`: 534 pass
- Pinned nightly clippy (the CI toolchain) clean across every changed crate; `fmt --check` clean. Two build failures I hit were `ld killed signal 9` (OOM linking giant test binaries in this 8 GB sandbox), not code defects — `cargo check --all-targets` is clean.
- Web: 771 pass; the 47 failures are **byte-identical to main** (verified via a main worktree with a fresh `bun install`) — pre-existing environment issues, zero new failures, and the new Forgejo panel test passes.

**Code review of the load-bearing pieces:** the new `fabro-forgejo` client (correct Forgejo wire shapes: `Do` merge field, `token` auth scheme, 201-on-PATCH close, `commit.id` branch heads, WIP-prefix drafts); the `PullRequestLink.forge` additive field with byte-identical legacy JSON round-trips proven at both the type level and the fabro-api parity level; host+subpath instance matching with hostile-host test cases (`forgejo.example.com.evil.test` does not match); token redaction in every `Debug`; fail-closed credential resolution (URL configured but token missing → error with remediation); install validates the token against `/api/v1/user` before writing anything; sandbox clone/push wiring through `InstallationTokenSource::pat` for both Docker and Daytona; provider routing in the PR pipeline, server handlers, and supervisor driven by the stored `forge` reference; demo routes mirrored; fork guarded with an explicit rejection; `auto_merge`+forgejo rejected at config resolution.

## Observations (non-blocking, for the record)

1. `FORGEJO_TOKEN` is injected into sandbox env whenever configured, even for GitHub-origin runs (the credential helper is origin-gated; the env var isn't). Mild least-privilege deviation from GitHub's opt-in injection model, consistent with the trusted-sandbox design.
2. `is_forgejo_origin` ignores ports — two instances on different ports of one host would both match. Edge case; failure mode is a failed clone, not a security hole.
3. Forgejo wire-format claims are fixture-tested, not live-verified against a real instance — the known, documented v1 tradeoff from the plan.
4. One new test uses `assert!(x == y)`; newer clippy than the pinned one flags this. Not a CI failure today.

None of these make the implementation wrong or require rework. The change does what the goal and the human's answers specified (PAT-only, single instance, new crate, core + operator surface), the tests would fail if the provider routing or wire shapes were wrong, and there is no regression versus main.

{"preferred_next_label": "approve"}