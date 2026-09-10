I attacked the plan against the repository as it actually is. One blocking defect survives scrutiny; a few secondary issues are worth naming but not blocking.

## Blocking defect: the run-admission layer is missing — Forgejo runs can never start

The plan's Step 5 modifies the sandbox clone contract (`clone_source.rs`) and Step 6 modifies workflow plumbing, but it never touches the layer that **decides whether a Forgejo-origin checkout may become a run at all**:

- `lib/components/fabro-manifest/src/lib.rs` — `observe_git_run_target` (line ~329) builds run targets via `github_run_target` (line ~417), which calls `fabro_github::parse_github_owner_repo` + `GitHubRepositorySlug` and returns `None` for any non-github.com origin. The doc comment states this exists "so clone-based execution can resolve the observed commit" — clone-based execution is driven by `RunTarget::Git`, not the legacy `GitContext`.
- `lib/apps/fabro-cli/src/commands/run/create.rs` (lines ~166-170) **enforces canonical GitHub run targets for the caller's checkout** — a Forgejo-origin checkout is rejected at `fabro run` time before any of the plan's sandbox/workflow code executes.
- `lib/foundation/fabro-types/src/run_intent.rs` — `GitRunTarget { repo }` stores a validated GitHub slug; `RunTarget::Git::validate()` (line ~83) rejects anything else ("must be a valid GitHub owner/name slug").
- Server-side `run_manifest.rs` / `run_compiler.rs` consume the same target.

So under this plan, the headline feature of the chosen scope — "sandbox clone from Forgejo" via the normal `fabro run` flow — is **dead on arrival**: the CLI rejects the checkout, `RunSpec` never carries a forge target, and the clone decision added in Step 5 is unreachable except through hand-constructed specs. All the sandbox/workflow work still compiles and its unit tests pass — this is also the "verification that would pass even if the change were wrong" failure mode. The only check that would catch it is the very last manual live smoke, after everything is built.

Worse, the fix forces a data-model decision the plan dodges: `GitRunTarget.repo` must learn to carry a forge origin (instance URL) — and that shape is **persisted** (automations table `target_repository`, run specs, `RunSummary`), making it expensive to reverse once shipped. The revision must also state the constraint the scope answer implies: **automation targets stay GitHub-only** (`automation_materializer.rs` validates slugs and automations were explicitly excluded), so the extension must not loosen validation on that path while accepting forge targets from local observation.

A revised plan needs: a step for `RunTarget::Git`/`GitRunTarget` carrying an optional instance origin (with round-trip tests in `fabro-types/tests/run_intent.rs` and `run_spec_methods.rs`), a forge branch in `observe_git_run_target`/`configured_repo_origin_url` (`run.scm.provider = "forgejo"` currently short-circuits to `None` at `fabro-manifest/src/lib.rs:434-443`), acceptance in `run create.rs`, passthrough in `run_manifest.rs`/`run_compiler.rs`, and a CLI-level integration test (e.g. `tests/it/cmd/runner.rs` or a scenario, using the twin) proving a forgejo-origin checkout reaches clone.

## Secondary issues (name them, but not blocking on their own)

1. **`normalize_repo_origin_url` host-genericity is assumed, not verified.** `observe_git_context` (`fabro-workflow/src/git.rs:107-116`) and manifest normalization pass every origin through `fabro_github::normalize_repo_origin_url`. If it mangles non-github SSH URLs, `GitContext.origin_url` is wrong for forge checkouts before anything else runs. The revision should verify this function's behavior on `git@forgejo.example.com:owner/repo.git` (likely fine — `ssh_url_to_https` looks host-generic — but it's load-bearing and cheap to check).
2. **Draft semantics are configurable server-side.** Gitea/Forgejo `WORK_IN_PROGRESS_PREFIXES` is instance-configurable; `WIP: ` may not mark a PR as draft on all instances. Not wrong work — the plan already documents it — but the docs step should say "prefix-based, may vary by instance" rather than promising draft behavior.
3. **Twin git smart-HTTP handler is quiet scope-add.** Justifiable (twin-github has one; clone tests need it) but it's the largest single item in Step 4 and only feeds optional docker tests. The revision should mark it explicitly optional/deferred so the implementer doesn't burn a day before the core is done.
4. **`repo init` forgejo probe and `demo/mod.rs` demo-mode status** are small surface items the plan names or half-names; fine either way, implementer judgment.

None of these four would force redoing written code; the missing admission layer would — it invalidates the plan's central claim that the run path works, and it embeds a persisted wire decision that is costly to reverse.

{"preferred_next_label": "revise"}