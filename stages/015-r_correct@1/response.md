# Correctness review — Forgejo integration (109 files, +5118)

I read the full diff and the surrounding context of every correctness-critical seam: the new `fabro-forgejo` crate (API ops, URL/credential helpers), `fabro-types` (origin matching, run-target validation, `PullRequestLink` wire format), the workflow PR pipeline dispatch, sandbox clone/push credential paths, server admission/preflight/handlers, the automation materializer and store, the CLI runner/doctor/repo-init, config resolution, and the web form. The forgejo-only paths are mostly sound. But the change breaks existing GitHub behavior in one high-impact way, and the automation path has two related defects.

## Finding 1 (High): Configuring Forgejo strips GitHub credentials from every CLI run

**File:** `lib/apps/fabro-cli/src/commands/run/runner.rs` — lines 141–148, 1123–1127, 1149–1175.

`requires_forgejo_credentials()` (line 1173) checks only `mode != DryRun && provider.is_clone_based()`. It never checks the run's origin — despite its own doc comment saying "whose origin belongs to the configured instance". `build_forgejo_context` then returns `Ok(Some(ctx))` for **any** clone-based run, and `forgejo.is_some()` is fed into `maybe_build_github_credentials`, which unconditionally returns `Ok(None)` at line 1125 ("GitHub credentials are neither required nor used").

Concrete triggers (both follow the setup documented in `docs/public/integrations/forgejo.mdx` — a `[server.integrations.forgejo]` block, which resolves as `enabled` by default):

1. **Token set:** run a GitHub-origin workflow via `fabro run` on Docker/Daytona. `github_app` = `None` end-to-end:
   - Publish fails for every auto-PR run: `publish.rs` explicitly errors `"pull request creation requires GitHub credentials"` when the target is GitHub and the context is `None`.
   - The Docker sandbox gets `github_app: None` → `build_token_source` returns `None` → no clone token (private-repo clones fail outright) and no run-branch push credentials.
   - `build_metadata_writer` returns `None` (neither the forgejo branch nor `github_app` matches) → metadata branch silently disabled.
   - Tools lose `GITHUB_TOKEN` (`built_env.github_token` derives from the same missing credentials).
2. **Token missing:** `build_forgejo_context` returns `Err`, and the `?` at line 142 fails *every* clone-based non-dry-run run — including GitHub ones — with "FORGEJO_TOKEN not configured". Merely adding the forgejo config block breaks all runs until the token is set.

The server-side path (`execute_run_in_process`, server.rs:4187) does this correctly — it gates on `origin_matches_instance`. Only the CLI worker gate is origin-blind. No test covers the "forgejo configured + GitHub run" combination, which is why this slipped through.

## Finding 2 (High): Automations with a Forgejo target can never be saved

**File:** `lib/components/fabro-automation/src/model.rs` — lines 165–172 and 400–410.

`validate_target()` and `validate_workflow_source()` call `RunTarget::validate()` / `GitRunTarget::validate()`, which are now `validate_with_scm(None)`. For `provider = "forgejo"` that returns `ForgejoUnconfigured` unconditionally. Every save path goes through it: `AutomationStore::replace` → `Automation::from_replace` (store.rs:118/133) → `normalize_replace` → `validate_target`, with no instance URL threaded in anywhere (the handler at `server/handler/automations.rs:235` passes the draft straight to the store).

Trigger: configure Forgejo exactly per the docs, then in the web UI create an automation and pick Provider = Forgejo (the select the diff itself adds at `automation-form.tsx:431`, shown precisely when forgejo is configured) → save fails with "forgejo targets require a configured Forgejo instance URL" even though the instance is fully configured. Same for a forgejo workflow source via the API. This makes the advertised "automations on Forgejo repos" scope unreachable and leaves the materializer's forgejo branches dead code.

## Finding 3 (Medium, latent — becomes live the moment Finding 2 is fixed): workflow-source remotes resolved with the target's forge

**File:** `lib/apps/fabro-server/src/automation_materializer.rs` — lines 279–283, 347–357; resolver at 167–176.

`forgejo_origin` is derived from the **target's** provider (line 282) but passed to `resolve_remote(CheckoutRole::WorkflowSource, repo, forgejo_origin)` at line 355 without regard to `source_provider` (computed at line 351, used only for the cache namespace at 359). In the resolver, `Some(origin)` unconditionally selects the forgejo clone URL. The `repo == &target_repo` shortcut (line 352) has the same flaw for same-slug/different-forge sources.

Trigger: automation target = forgejo `acme/widgets`, workflow source = `{ repo: "acme/widgets-docs", provider: "github" }` (the web form always submits sources without a provider tag, i.e. GitHub) → the resolver clones `acme/widgets-docs` **from the Forgejo instance**. If a same-slug repo exists on the instance, the automation silently reads workflows from the wrong repository; otherwise it fails with "repository not found". The reverse combo (GitHub target + forgejo source) mis-fails at line 300–303: `forgejo_origin` is `None` because the target is GitHub, so a fully-configured forgejo source dies with `ForgejoUnconfigured`.

## Minor (not blocking, listed for completeness)

- `fabro-forgejo/src/lib.rs:315` `find_open_pull_request` reads only the first page of `state=open` PRs (no `limit`); repos with more open PRs than the server's page size can miss the PR during reconciliation, degrading the recovery path to a failed create. Recovery-path degradation only.
- `run_manifest.rs` (~line 930): user-facing remediation contains a run of stray spaces: `"...run fabro install or run                  fabro secret set FORGEJO_TOKEN"`. Same class of stray-space typo exists in that message only.
- Preflight for an origin that is neither GitHub nor the instance (e.g. gitlab.com) now fails with "FORGEJO_TOKEN is not configured for a forgejo run" — misleading remediation; the old message named the actual restriction. Still fails, so no wrong result.
- `resolve/server.rs` `normalize_forgejo_instance_url` silently rewrites non-loopback `http://` to `https://` instead of erroring; an operator with an http-only LAN instance gets TLS failures against a URL they never wrote. Policy choice, but silent.
- Numerous `cargo fmt` violations will fail the CI formatting gate (`server.rs:4167` `{Ok(integration) => integration,`, `doctor.rs` `check_config {    match`, unindented `provider:`/`origin:` fields in `retry.rs:175`, `server/tests.rs`, `automation_scheduler.rs` tests, `fabro-automation` tests, `shared/forgejo.rs` missing trailing newline). Not logic bugs, but the change cannot pass `cargo +nightly-2026-04-14 fmt --check` as-is.

## What I checked and found sound

Origin/port matching (`origin_matches_instance`, incl. SSH-by-host and explicit-port semantics), the GitHub-first classification in `decide_clone`/`ScmTarget::resolve`/`run_target_for_origin`, `PullRequestLink` serde (GitHub links keep the historical 4-field shape; forgejo links require `origin`; `html_url` cross-check), `GitRunTarget.provider` wire defaults, the run-intent admission gate, PR live-detail/merge/close provider dispatch, the Docker/Daytona clone-and-lease machinery (forge-specific embed only at the two embed points; refresh path's HTTPS `set-url` receives normalized HTTPS origins), metadata-writer static-PAT provider, `fabro-forgejo` endpoint shapes (`Do` field, issues-endpoint close, branch `commit.id`), the additive SQLite migration with legacy-NULL → GitHub, and the server-side forgejo credential gating (origin-matched, fail-closed only at preflight-check granularity).

Summary: three findings — one breaks existing GitHub runs on forgejo-configured deployments (runner.rs), one makes forgejo automations unsavable (model.rs), and one silently clones workflow sources from the wrong host once the second is fixed (automation_materializer.rs). Everything else I traced behaved correctly.