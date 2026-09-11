I've now covered the full diff: the new crate, types, sandbox, workflow pipeline, server handlers, CLI, manifest, config, store, automation, web UI, and generated clients. Here is my complexity-only review.

# Complexity review — Forgejo integration (111 files, +5637/−464)

Overall the change follows the plan's shape (parallel crate, additive provider tag) and reuses the right shared seams (`origin_matches_instance` has ~13 call sites and is correctly centralized in `fabro-types`; `repo_layout` unifies the sandbox checkout layout; `RepoLayout` rename is right). The findings below are where the change carries weight it doesn't use.

## Findings

**1. `CloneDecision::{GitHub, Forgejo}` — two variants with identical payloads that no consumer distinguishes.**
Both variants hold exactly `{ origin_url, branch, tag, commit_sha }`. Every production match site combines them into one identical arm (`docker.rs:1892`, `daytona/mod.rs:1600`, `clone_source.rs:414`), and then the GitHub-vs-Forgejo decision is *re-derived* downstream anyway (`is_forgejo` via `origin_matches_instance` in `docker.rs:934`, again in `daytona_repo_layout`). The split carries zero information. Simpler: one `CloneDecision::Clone { origin_url, branch, tag, commit_sha }` — ideally with the layout (or at least an `is_forgejo: bool`) computed once in `decide_clone` and passed through, which also deletes the second and third classification per provider. The unspittable-URL error message can stay where it is.

**2. The supervisor path cannot serve Forgejo, and the request shape invited the bug.**
`pull_request_supervisor.rs:188-189` hardcodes `forgejo: None`. If a forgejo-origin run's PR creation reaches the supervisor (recovery/async fulfillment after the in-pipeline publish didn't complete), `open_pull_request` fails with "origin … is not a GitHub repository and no configured Forgejo instance matches it" — a misleading durable failure. This is the concrete cost of `OpenPullRequestRequest { github: Option<_>, forgejo: Option<_> }`: two optional credential slots where exactly one is valid, resolved independently at each caller (`publish.rs`, supervisor). Either wire the forgejo context here, or make the GitHub-only constraint explicit (skip/annotate forgejo creations at the top of the supervisor) so the failure mode is intentional instead of accidental.

**3. Empty marker feature: `fabro-forgejo/src/test_support.rs` + `test-support = []`.**
The file is two lines of comment; no crate enables `fabro-forgejo/test-support` (verified across all Cargo.tomls). Delete the file, the `#[cfg(any(test, feature = "test-support"))]` module declaration, and the `[features]` section. Re-add when there's something to put in it.

**4. Dead public parse entry points on `PullRequestLink`.**
This diff adds `from_forgejo_url` with zero production callers (tests only), and orphans `from_github_url` (its only caller, `handler/pull_requests.rs`, switched to `from_url`). Keep the single `from_url` entry point plus the private `*_pull_request_link_from_url` helpers; make the two wrappers private or `#[cfg(test)]`.

**5. `check_forgejo_token` in `diagnostics.rs` re-implements `fabro_forgejo::current_user`.**
It hand-rolls the `GET /api/v1/user` probe with its own header set and status mapping (~50 lines), while the crate ships exactly that probe — currently consumed only by `tests/live_access.rs`. Call `current_user` with `state.http_client()` and map the outcome to `CheckResult`. Deletes the duplicate and unifies the auth-failure wording (which currently differs between the two).

**6. Three copies of "load default server settings → extract forgejo instance URL."**
`run_tool_manifest.rs::forgejo_instance_url_from_settings`, `shared/repo.rs::forgejo_instance_url_matching`, and an inline copy in `repo/init.rs::forgejo_scm_block` all repeat the same 5–8 line `ServerSettingsBuilder::load_default()` dance. Two of the three are in fabro-cli; all three would collapse into one helper (a `fabro_config`-level `forgejo_instance_url_from_default_settings()` fits, since all three crates already depend on it).

**7. `repo/init.rs` writes `project.toml` twice with the template duplicated in the same function.**
The file is written with the bare template, then conditionally rewritten with the same template plus the `[run.scm]` block — two copies of the heredoc that will drift. Build the string once (`content.push_str(scm_block)`) and write once.

**8. `normalize_forgejo_instance_url` / `is_loopback_instance_url` — parsing complexity whose only effect is a silent rewrite.**
~20 lines of hand-rolled host/port extraction (a third copy of the logic that `fabro_types::url_host_port`, added in this same diff, already implements) exist to decide whether to silently rewrite `http://` → `https://`. For an operator with a plain-HTTP instance on a LAN host (realistic for self-hosted Forgejo), this mangles the configured URL into TLS failures with no hint. Simpler and more honest: trim the trailing slash and use the URL as configured, or reject plain-HTTP non-loopback with an explicit error — no rewriting, no loopback detection.

## Minor notes (one-line fixes, fold into a pass)

- `run_manifest.rs:984` — `let _ = access.default_branch;` is a speed bump; match `Ok(Some(_))` instead of binding the struct to discard a field.
- `ForgejoIntegrationSettings.enabled` is behaviorally redundant with `url` presence (both `None`-paths produce "unconfigured"), and the documented example (`forgejo.mdx:29`) teaches `enabled = true`, which is already the default. It's documented, so keep if the kill switch is wanted — but it's one more knob nobody needs in v1.
- `RepositoryProvider::Forgejo` is produced by `repository_provider_with` but never read except when serializing `RepositoryRef.provider`; all real decisions flow through `ScmProvider`. Two parallel provider taxonomies now exist — acceptable as display data, but remember it when the noted "provider-neutral RepositorySlug" refactor happens.
- Formatting slips the fmt gate will flag (other reviewers own correctness, flagging since it's mechanical): column-0 `provider:` lines at `retry.rs:175` and `start.rs:2095/3394/3422/3445/3463`, mis-indented `provider:` lines in `automation_scheduler.rs` tests and `model.rs` tests, and the `cache_namespace: None` indent in `git_checkout.rs` tests. `cargo +nightly-2026-04-14 fmt --all` fixes all of them.

## Appropriately simple

For the record: the `fabro-forgejo` crate itself is lean (no unused API after the review-fix pass; `get_pull_request` correctly reuses the GitHub-shaped detail type instead of minting a parallel one); the `ScmTarget` dispatch in the PR pipeline is flat matches, not a premature trait; `AuthProvider::username()` with a default is the minimal way to thread the git username; the automation SQL columns, migration, OpenAPI/TS-client churn, and web UI changes are all additive and proportionate; and the event/provider fields follow the established serde-default compatibility pattern.

The change is close to appropriately simple — findings 1–5 are the ones I'd fix before merge; 6–8 are cheap collapses a single pass can absorb.