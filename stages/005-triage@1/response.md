I have enough to plan. Here's my understanding of what you want, including the decisions I'm making on your behalf:

**What you want:** Forgejo support as a peer to the existing GitHub integration — Forgejo-hosted repositories can serve as run origins and as the target for pull-request-style automation, instead of GitHub being the only supported forge.

**Decisions I'm making on your behalf (flag if wrong):**

1. **Transport:** Forgejo's Gitea-compatible REST API v1 (`/api/v1`), authenticated with personal access tokens (Authorization header). No JWT/installation-token app flow — Forgejo doesn't have GitHub App semantics, and token auth is the standard integration path there.
2. **Scope of clone support:** Forgejo origins become valid clone sources for the clone-based sandbox providers (Docker, Daytona), lifted out of the current "GitHub-only, otherwise fail unless `skip_clone`" contract, with the same branch/commit rules as GitHub. This is the material change; without it the integration would be cosmetic.
3. **Config:** Instance base URL + token per deployment (env/config, mirroring how GitHub credentials are held), so any Forgejo instance (including self-hosted and Codeberg) works — no hardcoding of hosts, and origin detection is "is this host a configured Forgejo instance," not domain-pattern sniffing.
4. **PR automation parity:** create/update PRs and comment on them via the Gitea-compatible endpoints, following the same shape as `fabro-github`'s PR creation, behind the same workflow hooks. Anything GitHub-specific with no Forgejo equivalent (App installations) is simply absent, not stubbed.
5. **Non-goal for now:** migrating existing GitHub behavior; GitHub remains the default and untouched.

If any of those five is materially wrong, it's cheap to correct during review — but they follow directly from your answers and the existing architecture, so I'm proceeding to plan.

{"preferred_next_label": "enough"}