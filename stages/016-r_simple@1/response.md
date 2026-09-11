Review complete. I read the full diff (`main...HEAD`, 109 files) plus the surrounding context of every seam it touches. The overall shape is disciplined — `ScmTarget`, `PushTokenSource`, `CloneDecision::Forgejo`, the dual-shape `PullRequestLink` serde, the config layer, and the migration all reuse existing machinery rather than inventing parallel structures. But there are real complexity findings, two of which are flag-threading that is both heavier than the alternative *and* behaves incorrectly.

## Findings

### 1. The CLI runner credential gate is origin-blind — and the `forgejo_run` bool it threads exists only because of that

`lib/apps/fabro-cli/src/commands/run/runner.rs`

`maybe_build_forgejo_context` gates on `requires_forgejo_credentials` = "non-dry-run + clone-based" — it never checks that the run's origin belongs to the Forgejo instance. It was copied from `requires_github_credentials`, which could afford to be origin-agnostic because every clone-based run used to be a GitHub run; the forgejo copy dropped the one discriminator that matters. Consequences on any deployment with `[server.integrations.forgejo] url` set:

- Token present → every **GitHub-origin** clone-based CLI run gets `forgejo = Some`, and the new `forgejo_run: bool` parameter then makes `maybe_build_github_credentials` return `Ok(None)` before its own gates. The run silently loses GitHub credentials; PR creation later fails with "pull request creation requires GitHub credentials".
- Token absent → every GitHub-origin clone-based run hard-fails demanding `FORGEJO_TOKEN`.

The `forgejo_run: bool` plumbed into `maybe_build_github_credentials` is the tell: it exists solely to encode the coupling that the missing origin check would have made unnecessary. The simpler and correct version classifies once by origin — `run_spec.repo_origin_url()` + `origin_matches_instance`, the exact mechanism this same diff uses in the server admission path, preflight, `initialize.rs`, `publish.rs`, and `run_metadata.rs` — builds the forgejo context only for instance origins, and deletes the bool parameter.

### 2. `automation_materializer.rs` derives `forgejo_origin` from the *target's* provider and then uses it for the *workflow source* — broken in both mixed directions

`lib/apps/fabro-server/src/automation_materializer.rs`

```rust
let forgejo_origin = self.forgejo_instance_url
    .as_deref()
    .filter(|_| input.target.provider == ScmProvider::Forgejo)   // target-gated
```

Since `AutomationGitWorkflowSource = GitRunTarget` and the API/store persist an independent `workflow_source_provider`, mixed combinations are representable, and both are wrong:

- Forgejo source + GitHub target → `source_origin` collapses to `None`, so `validate_with_scm(None)` fails with "forgejo targets require a configured Forgejo instance URL" even though the instance *is* configured.
- Forgejo target + GitHub source → the source passes validation, but `resolve_remote(WorkflowSource, repo, forgejo_origin)` receives the target-derived `Some(origin)` and builds `https://<instance>/{github_owner}/{github_repo}.git` — a clone URL on the wrong forge.

The simpler version deletes both provider filters: pass `self.forgejo_instance_url` unconditionally (`validate_with_scm` already no-ops it for GitHub targets — see `run_intent.rs`), and have `resolve_remote` branch on the *remote's own* provider rather than a target-derived `Option<&str>`. The `cache_namespace_for` closure then keeps only its per-remote provider filter, which is the one that's actually correct.

### 3. Dead code cluster in `fabro-forgejo`

`lib/components/fabro-forgejo/src/lib.rs`, `src/test_support.rs`, `Cargo.toml`

- `unused_http_client_helper()` — an empty function under `#[expect(dead_code)]` whose comment says "call sites arrive with the run-pipeline wiring." The wiring is this diff; it arrived; nothing calls it. Delete.
- `HttpMethod::Put` — never constructed anywhere in the crate (merge is POST, close is PATCH; the implementer's own summary concedes this). It survives only as a match arm. Drop the variant.
- The `test-support` feature + `src/test_support.rs` — a feature flag nobody enables and a module that is two comment lines calling itself "a marker." This violates the repo's own rule that test-support features exist only when another crate's tests consume them via a dev-dependency. Delete the feature, the `#[cfg]`, and the file; the crate's own tests use `#[cfg(test)]` and are unaffected.

### 4. The git-credential-helper surface has zero production callers

`FORGEJO_CREDENTIAL_HELPER`, `credential_helper()`, `credential_helper_key()` (`src/lib.rs`, `src/url.rs`)

The doc comment claims three consumers ("the runtime git bridge, server preflight probes, and live contract tests"). None exist: the runtime clone path embeds the token via `embed_token_in_url`, preflight uses `get_repo` + `forgejo_git_auth`, and `git_bridge.rs` was deliberately left unchanged (recorded as adaptation #4). Nothing outside the crate references any of the three names, and `credential_helper()` is additionally a pure alias of the const — two public names for one thing. Delete the whole surface until a consumer actually lands; it's ~40 lines plus tests of speculative API.

### 5. `current_user` is unused production code whose only purpose is duplicated in server diagnostics

`fabro_forgejo::current_user` ("Verifies the PAT by reading the authenticated user") has no production caller — only its own tests. Meanwhile `check_forgejo_token` in `lib/apps/fabro-server/src/diagnostics.rs` hand-rolls the identical probe: same `/api/v1/user` path, same `Authorization: token` header, its own status mapping, ~50 lines. Collapse to one copy: either call `current_user(&http, &ctx)` inside the diagnostics `timeout(...)` and delete the inline request building, or delete `current_user` if diagnostics keeps its raw probe.

### 6. `GET /repos/forgejo/{owner}/{name}` has no consumer in this change

`lib/apps/fabro-server/src/server/handler/system.rs` (+~130 lines), plus the OpenAPI path and generated TS client.

The GitHub twin has a caller (`repo init`'s installation check). The forgejo version has none: no web repo picker landed in this diff (the automation form validates `targetRepository` locally), and `repo init`'s forgejo branch works offline. The stated scope included a web repo picker; the endpoint is the server half of a feature whose client half doesn't exist. Either wire the consumer or defer the endpoint, spec entry, and regenerated client together.

### Minor

- `repo/init.rs` writes `.fabro/project.toml`, then conditionally rewrites it wholesale with the scm block. Compute `forgejo_scm_block` first and write once.
- `draft_title_prefix` is a `pub` fn with one internal caller plus a dedicated assertion-only test; inline it (or at minimum drop the `pub`).
- `open_pull_request` calls `target.slug()` twice per field (`target.slug().0`, `target.slug().1`) when building the link; bind it once.

## Appropriately simple

For the record: the `ScmTarget` enum, `PushTokenSource` wrapping the existing `InstallationTokenSource`, `CloneDecision::Forgejo` sharing the GitHub match arm, `PullRequestLink`'s dual wire shape (old readers keep working), the `provider` serde defaults on `GitRunTarget`/events, `validate_with_scm` threading, the config layer + loopback HTTP rule, the additive SQL migration, and the web changes are all proportionate. Nothing in the diff exceeds the agreed tier-b scope. Findings 1 and 2 are the ones I'd treat as blocking — they're not just extra code, they change behavior for mixed GitHub/Forgejo deployments and mixed-provider automations.