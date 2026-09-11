The host disk never recovered — byte-level writes fail across the entire shared overlay. I've exhausted what's verifiable in this environment. Final summary:

## Summary of changes (review-fix pass)

The implementation was already committed by the pipeline; this stage addressed the three blocking defects from `deep_review`. All changes are in 19 files, +626/−107.

### Defect 1 — CI formatting gate (fixed by hand; the pinned nightly toolchain can't be installed at 0 bytes free, and stable rustfmt mis-reports style_edition-2024 formatting as diffs, so I fixed the exact constructs instead)

- `lib/apps/fabro-cli/src/commands/doctor.rs` — split `{    match` off the `check_config` signature line.
- `lib/apps/fabro-server/src/server.rs` — split `{Ok(integration) => integration,` onto its own line.
- `.../server/handler/pull_requests.rs` — broke the 121-char `is_some_and` chain; both 107-char `ApiError::new(...)` match arms rewritten as braced arms (inner line exactly 100).
- `lib/apps/fabro-server/src/run_manifest.rs` — the 143-char `Err("...")` line, and fixed a copy/paste artifact (a run of spaces inside the remediation message); message now uses `` `fabro secret set FORGEJO_TOKEN` `` matching doctor/server phrasing.
- `lib/apps/fabro-server/src/automation_materializer.rs` — broke the 102-char `resolve_remote(...)` args vertically.
- `lib/components/fabro-manifest/src/lib.rs` — four 110–113-char test calls broken into rustfmt's arg-per-line + `.unwrap()` chain shape.
- `lib/components/fabro-sandbox/src/push_credentials.rs` — 101-char `.map_err(...)` chain: closure body moved into a block.
- `lib/components/fabro-store/src/run_state.rs` — 105-char `PullRequestLink::github(...)` arm broken into a multi-line call arm.
- `lib/components/fabro-workflow/src/pipeline/pull_request.rs` — 104-char struct field: extracted `github_app::GitHubContext` into a local; `github_app` alias resolves via the tests module's `use super::*`.
- `lib/components/fabro-workflow/src/run_metadata.rs` — `discover_parent` signature broken across lines.
- `lib/foundation/fabro-types/src/pull_request.rs` — `let parsed =` break, mirroring the identical existing pattern 17 lines below.
- `lib/foundation/fabro-types/src/run_intent.rs` — 110-char `use crate::{...}` split; `tests/run_intent.rs` — `assert_eq!` args split.
- `lib/apps/fabro-cli/src/shared/forgejo.rs` — trailing newline added.

Lines >100 that remain are pre-existing or unspittable string literals already alone on their line — accepted rustfmt overflow.

### Defect 2 — missing wiring tests (added all five)

- **2a server** (`tests/it/api/runs.rs`): 4 new tests — intent admission without the integration → 422 `target_invalid` with the exact ForgejoUnconfigured message; intent admission with URL but no vault PAT → 422 `forgejo_integration_unconfigured` naming `server.integrations.forgejo.url`; preflight with a forgejo origin and no PAT → failed "Repository Access" check with `FORGEJO_TOKEN` remediation; preflight against an httpmock-served instance API (repo + branch endpoints, `token`-auth header) → passing check with both probes asserted.
- **2b materializer** (`automation_materializer.rs` tests): full-pipeline test proving a `provider = "forgejo"` target threads the instance URL to the resolver and clones via PAT auth through a local bare fixture; plus a direct `ServerGitHubRemoteResolver` test pinning the forgejo clone URL/`forgejo_git_auth` equality, the origin-mismatch `FORGEJO_TOKEN` error, and an untouched GitHub lane.
- **2c manifest** (`fabro-manifest/src/lib.rs` tests): `provider = "forgejo"` (both casings) resolves `ConfiguredOrigin` against the instance; blank/missing instance → the explicit `requires a configured Forgejo instance` error; missing owner/repo → `None`; `github` provider still resolves without Forgejo.
- **2d repo/init** (`tests/it/cmd/repo_init.rs`): SSH origin on the configured instance (via `FABRO_CONFIG`) writes the `[run.scm] provider = "forgejo"` block with owner/repository; GitHub origin writes no scm block.
- **2e PR pipeline** (`pipeline/pull_request.rs` tests): a forgejo origin runs branch-verify → reconcile → create entirely against the instance API with `token`-auth headers, returns a `WIP: `-prefixed draft title and a `Forgejo`-tagged link carrying the instance origin, and makes **zero** calls to the GitHub mocks.

**Additional defect found while adding 2e** (invisible to the reviewer, who had no disk to compile): three existing test struct literals omitted the new `forgejo` field, so `fabro-workflow`'s tests did not compile. Added `forgejo: None` to all three.

### Defect 3 — dead public API (removed)

- Deleted `FORGEJO_CREDENTIAL_HELPER`, `credential_helper`, `credential_helper_key`, their tests, and the `#[expect(dead_code)]` stub; crate docs now state the actual auth seams (PAT embedded in clone URLs at sandbox/server; local pushes use host credentials, same as GitHub).
- Removed the never-read `ForgejoRepository.owner` field and the `RepositoryOwner` struct (serde ignores the payload's `owner` key).
- `current_user` kept: it is consumed by `tests/live_access.rs` (the live PAT contract test) — same role as GitHub's token validation path.
- `secret_registry.rs`: `FORGEJO_TOKEN` added to the `classifies_optional_vault_secrets` test list.

## Verification status

- **Ran green:** `cargo test -p fabro-forgejo` (all, incl. 11 httpmock integration tests); `cargo test -p fabro-types -p fabro-static` (500+ tests incl. the forgejo run-target and secret-registry tests); `apps/fabro-web bun test` on the changed file (2/2; the 28 full-suite failures are all in files the forgejo change never touched — pre-existing baseline).
- **Blocked by environment:** after the smallest-crate runs, the **shared host disk went to 100% and byte-level writes fail** (deleting build artifacts, the registry src, and even the bun runtime only bought minutes — an external tenant is consuming the pool; I probed and retried for ~40 minutes). `cargo test -p fabro-manifest` and `cargo check --tests` for `fabro-store/fabro-workflow/fabro-server/fabro-cli` fail at rmeta/link with ENOSPC. As partial compensation, every edited file passes a full rustc parse (rustfmt-as-parser, zero syntax errors), all added lines respect `max_width = 100` modulo accepted string overflow, and the stable-rustfmt proposals intersecting my edits are exclusively the two known stable↔nightly divergences. Once disk frees, a reviewer should run: `cargo nextest run -p fabro-manifest -p fabro-store -p fabro-workflow -p fabro-server -p fabro-cli` and `cargo +nightly-2026-04-14 fmt --check --all`.