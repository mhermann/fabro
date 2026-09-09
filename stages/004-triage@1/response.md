I have enough to plan. Here's what I understand the human wants, including the decisions I'm making on their behalf:

**Goal:** Add Forgejo as a git-forge integration alongside the existing GitHub integration, following the same shape as `fabro-github`.

**What I'll build:**
- A new `fabro-forgejo` crate mirroring `fabro-github`: token-based auth, API client against Forgejo's Gitea-compatible v1 API (repo access, branches/commits, PR create/list/comment, statuses), with an instance base URL required since Forgejo is self-hosted.
- Configuration via env vars consistent with existing conventions (e.g. `FABRO_FORGEJO_URL`, `FABRO_FORGEJO_TOKEN`), wired wherever GitHub credentials are resolved.
- Clone-source support: extend the sandbox clone contract so run manifests with a Forgejo origin (HTTP clone with token) are admitted, keeping today's GitHub path byte-identical; other non-GitHub origins keep failing unless `skip_clone = true`.
- Surfaces that currently reference GitHub (run intent admission, PR creation hooks, server/CLI config) get a provider selection between GitHub and Forgejo.

**Decisions I'm making myself:**
- Keep `fabro-github` untouched rather than refactor both behind a shared trait now — I'll introduce only the minimal internal dispatch point needed for the workflow/server to pick a provider. A full unification can be proposed separately if drift becomes painful.
- Document and test that exact-commit handling follows the same branch/SHA contract as GitHub clones, without adding ancestry verification in the sandbox layer.

The remaining unknowns (which Forgejo version to target, whether webhooks are in scope now) are small enough to resolve during planning without another round.

{"preferred_next_label": "enough"}