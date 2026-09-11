I've now read the full diff and the surrounding context of every major surface. Here is the complexity-only review.

## Verdict

The change is largely proportionate to the goal. The parallel-crate shape, the explicit `PullRequestHost` dispatch, the twin, and the accepted duplication of the HTTP-client trio / URL-normalizer trio / `SecretString` from `fabro-github` were all explicitly decided during scoping, so I am not re-reporting them. The threadings (`forgejo` field alongside `github_app` at each hop) mirror the existing GitHub pattern exactly. What follows are the findings that are *not* covered by those accepted decisions — two of them are dead code, the rest are collapsible duplication.

## Findings

**1. `TwinForgejo` fixture in `fabro-test` is dead code, plus a dependency added only for it.**
`lib/foundation/fabro-test/src/lib.rs` adds a `TwinForgejo` struct (~25 lines) and re-exports `ForgejoAppState`, and `lib/foundation/fabro-test/Cargo.toml` adds the `twin-forgejo` dev-dependency. Nothing uses them: the only consumer of the twin, `test/twin/forgejo/tests/client_ops.rs`, uses `twin_forgejo::{AppState, TestServer}` directly. Compare `TwinGitHub`, which has two real consumers (`fabro-cli` auth harness, `fabro-github` integration tests). Delete `TwinForgejo`, the re-export, and the dependency until an e2e test actually needs the fixture — the twin's own test target already exercises everything.

**2. `.env.example` documents live-test variables that no code reads.**
It advertises `FABRO_TEST_FORGEJO_URL` / `FABRO_TEST_FORGEJO_TOKEN` as gating "opt-in Forgejo live tests … `--profile e2e --run-ignored only`", but a repo-wide search finds zero references to either variable in any `*.rs` file. The promised env-gated live tests don't exist. Either delete the `.env.example` entry, or this is missing scope (the plan's test strategy leaned on these tests existing). As written it's configuration nobody can set to any effect.

**3. `build_forgejo_config` has an unreachable parameter branch.**
`lib/apps/fabro-cli/src/shared/forgejo.rs` takes `settings: Option<&ForgejoIntegrationSettings>`, but both call sites (`runner.rs:149`, `resolve_forgejo_config`) pass `None`. The settings-URL branch and the `settings.is_some_and(|s| !s.enabled)` check inside can never execute. Simpler version: drop the parameter and resolve from `FORGEJO_URL`/vault only (which is what the comment at the runner call site already says happens), or actually load server settings at the runner site if the parameter was the intent.

**4. Two Forgejo resolvers in the same file.**
`server.rs` contains `AppState::forgejo_config` (async, one caller: `diagnostics.rs`) and `resolve_startup_forgejo` (sync, one caller), each re-implementing enabled → trim URL → `ForgejoInstance::new` → read vault token → trim → build `ForgejoConfig`, with divergent error behavior (one `Err`s on a bad URL, the other warns and disables). Collapse the shared part into one pure helper over `(enabled, url, Option<token>)` and keep the two thin callers for their distinct error policies. Today a future change to resolution rules must be made twice in one file.

**5. CLI `pr link` re-derives information the server response already carries, via an extra vault open.**
`commands/pr/link.rs` calls `forgejo_link_for_url` before even resolving the run: it opens the secret store, resolves config, and re-parses the URL — all to pick the label `"forgejo #N"` vs `"github #N"`. But the server classifies the URL (`pull_request_record_from_link_request`) and returns the record with `forge` set, which `record_label` itself already falls back to reading (`forgejo_link.unwrap_or(record)` then `link.forge.is_some()`). The local classification can only agree with the record or with a server config mismatch that errors out anyway. Delete `forgejo_link_for_url` and make `record_label` read `record.forge` directly.

**6. Minor: `ForgejoRepository` fields nobody reads.**
The only production caller of `get_repository` (`run_manifest.rs:947`) discards the struct with `.ok_or_else(...)`; `full_name`/`private`/`fork` are read only by the crate's own unit test. Return `Option<()>`-equivalent (or at least drop `private`/`fork`) until a consumer exists.

**7. Minor: `ForgejoInstance`'s manual `Serialize`/`Deserialize` are test-only.**
No production type embeds a `ForgejoInstance` — settings persist the URL as `Option<String>`, and `ForgejoConfig` (holding a secret) is never serialized. The ~20-line serde pair is exercised only by `src/tests.rs`. Fine to keep if a persisted config type is planned, but today it's production surface with a single (test) caller.

## Not findings (checked and cleared)

- `CloneDecision::Forge` duplicating the `GitHub` variant shape, `CloneOrigin` in docker.rs, and the `CloneRepoLayout` rename — mechanical dispatch per the accepted architecture.
- The web install step, CLI install wizard step, diagnostics check, install endpoints, and system-integrations status all mirror their GitHub counterparts 1:1 and are explicitly in scope B.
- `validate_forgejo_token` hand-rolling the `/api/v1/user` call in `install.rs` duplicates `get_authenticated_user` from the new crate, but it exactly mirrors the adjacent pre-existing `validate_github_token` pattern — borderline, reported only as a note.
- The twin (2,114 lines) is proportionally *smaller* than twin-github (4,829 lines).
- Event/store/OpenAPI additions (`PullRequestLink.forge`, `PullRequestCreatedProps.forge`) are the minimal wire changes the feature requires, with byte-identity preserved for old records.

Items 1–5 are the ones worth acting on; 1, 2, and 5 are straightforward deletions.