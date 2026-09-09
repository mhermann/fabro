All four questions answered. Restatement for the planning stage:

**1. V1 scope → Core + operator surface**
Forgejo support covers: clone from Forgejo origins, push branches/checkpoints, auto-PR create/merge/close, `run.scm.forgejo`, integration status in the web UI + system endpoint, a repo-access check endpoint (`/api/v1/repos/forgejo/...`), and `fabro install` support for a Forgejo token strategy. **Webhook ingestion and automation triggers are explicitly out of scope for v1** — no new webhook route, no automation-materializer changes.

**2. Auth model → PAT/token only**
A scoped API token stored like GitHub's `token` strategy (env/vault, e.g. `FORGEJO_TOKEN`). No OAuth2 app, no browser sign-in via Forgejo — web UI auth is untouched. No GitHub-App-equivalent flows, so no installation-token machinery for Forgejo.

**3. Instance model → Single configured instance**
One instance URL + one token (e.g. `FORGEJO_URL`, `FORGEJO_TOKEN`). `owner/repo` slugs remain host-free and unambiguous, credential lookup stays simple, and the git credential helper scopes to that single configured host. Multi-instance/host-aware slugs are out of scope.

**4. Crate architecture → New fabro-forgejo crate**
A parallel component crate (`lib/components/fabro-forgejo`) with its own client and DTOs, following the `fabro-github`/`fabro-slack` pattern. The existing GitHub path is not refactored; shared behavior is limited to foundation helpers. Duplication of URL/token-caching patterns is an accepted cost.

Decisions I had already made and that stand unless overridden: no tracker support; naming follows convention (`run.scm.forgejo`, `integrations.forgejo`, `RepositoryProvider::Forgejo`, provider string `"forgejo"`); PR links accept `{instance}/owner/repo/pulls/{n}` with `/pull/{n}` as alias; sandbox token env var is `FORGEJO_TOKEN` (no `GITHUB_TOKEN` overload); target is the Forgejo API — Gitea instances likely work but are not promised.

The planning stage can now treat the goal as fully determined: a single-instance, PAT-only, new-crate Forgejo integration covering core SCM plus the operator surface, with webhooks/automations and OAuth left for a later pass.