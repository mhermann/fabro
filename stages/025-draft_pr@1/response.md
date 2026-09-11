Both files are written:

**`/tmp/pr-title.txt`** (52 chars)
> Add Forgejo integration for pull requests and clones

**`/tmp/pr-body.md`** covers the change as it stands today:

- **New `fabro-forgejo` crate** — REST v1 client (PR create/find/merge/close, auto-merge via `merge_when_checks_succeed`, repo/branch/user reads), origin helpers, validated `ForgejoInstance` (HTTPS-only in prod, subpath + port support), redacted credential types, and an instance-scoped git credential helper so the PAT stays out of git config/argv/errors.
- **Config & secrets** — `[server.integrations.forgejo]`, vault `FORGEJO_TOKEN`/`FORGEJO_WEBHOOK_SECRET`, registered in the secret registry, run-level `token = true` sandbox injection (exposure-only, since Forgejo PAT scopes are fixed at creation).
- **Sandboxes** — new `CloneDecision::Forge` lane for Docker and Daytona with identical clone sequence to GitHub.
- **Pipeline** — `GitHub | Forgejo` PR-host enum so create/reconcile/merge/close/auto-merge flows work unchanged; forge PRs reuse `PullRequestGithubDetail` with a `provider` discriminator.
- **Surfaces** — inert HMAC-verified webhook route, install API endpoints + `fabro install` step, `fabro doctor` check, CLI `pr link`, web install flow, OpenAPI-first types via `with_replacement` with round-trip tests, docs page + changelog.
- **Review finding fixed** — the GitHub scp-like normalizer was swallowing instance ports (`host:3000/owner/repo`), misrouting preflight; dispatch now reads the raw origin and Forgejo normalization preserves numeric ports, with regression tests on both the normalizer and preflight.
- **Verification** — I re-ran the affected suites just now: `fabro-forgejo` (35), `twin-forgejo` (5, client ops against the in-process twin over HTTP), `fabro-sandbox` lib (133), `fabro-server` run_manifest (46), `fabro-workflow` pull_request (43) — all passing.