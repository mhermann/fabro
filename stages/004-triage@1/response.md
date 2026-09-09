I have enough to plan. Here's my understanding of what's wanted, plus the decisions I'm making on the human's behalf:

**What they want:** Forgejo support comparable to the existing GitHub integration — the ability to point Fabro at Forgejo-hosted repositories (codeberg.org out of the box, self-hosted instances via configured base URL) for the same jobs GitHub does today: authenticated API access, PR creation/comments, and clone-based run workspaces.

**Decisions I'm making myself:**

- **Structure:** a new sibling crate `lib/components/fabro-forgejo` mirroring `fabro-github`'s surface (auth, API client, PR operations), rather than prematurely generalizing `fabro-github` into a generic "git forge" abstraction. This matches the existing one-integration-per-crate pattern (`fabro-github`, `fabro-slack`, `fabro-mcp`).
- **Auth:** Forgejo API tokens (personal access tokens) — Forgejo has no GitHub-App JWT/installation-token equivalent, so token auth is the honest parity point, with per-instance token config in server/CLI config mirroring how GitHub creds are supplied.
- **Clone support:** extend the sandbox clone-source contract so run manifests with a Forgejo origin (host + owner + repo) clone successfully in both Docker and Daytona providers, reusing the exact branch/SHA semantics and never-fallback rules already documented for GitHub origins. GitHub-only behavior is untouched for GitHub origins.
- **Webhooks:** treat Forgejo payloads as Gitea-compatible; parse the event types needed for run triggering, mapping them onto the same internal events GitHub webhooks produce.
- **Scope boundary:** issue timelines, releases, and other GitHub features with no clear role in Fabro's run flow stay out of the first pass.

Nothing in their answers opened a gap that would change *what* gets built — the rest is implementation detail I can settle while planning.

{"preferred_next_label": "enough"}