I have enough to plan. Restating what I understand the human wants, plus the decisions I'm making on their behalf:

**Goal:** Add Forgejo support mirroring the existing GitHub integration, spanning the same surface GitHub currently occupies in this codebase.

**What I'll build (as understood):**

1. **New `fabro-forgejo` crate** paralleling `fabro-github` — API client for PR creation and repo operations against a Forgejo instance's REST API (Gitea-compatible shape).
2. **Auth**: token-based. Forgejo has no GitHub Apps equivalent, so the faithful analogue of "installation token" is a scoped personal access token. I'll support instance URL + token (env/config following the existing GitHub settings patterns), not an OAuth2 web flow.
3. **Self-hosted instance URL**: unlike the fixed `github.com`, Forgejo requires a configured base URL per instance — this goes into config (`FABRO_FORGEJO_URL` / `FABRO_FORGEJO_TOKEN` style, matching existing conventions).
4. **Sandbox clone sources**: extend `fabro-sandbox` so Docker and Daytona providers accept Forgejo origins (currently non-GitHub origins fail unless `skip_clone = true`). Forgejo clones use HTTPS with token auth; the branch/exact-commit contract (shallow fetch of SHA, verify HEAD, never fall back to branch HEAD) stays identical. Origin detection distinguishes Forgejo hosts from GitHub rather than treating any Git URL as cloneable.
5. GitHub paths remain untouched; no speculative refactor of `fabro-github` into a generic forge abstraction — the two crates stay separate unless a clean shared type emerges naturally in a shared crate.

**Decisions made on the human's behalf:** PAT-first auth (no OAuth2 dance in v1), including sandbox clone support in scope since clone-based providers rejecting non-GitHub origins is half the integration, and keeping exact-commit admission branch-only for Forgejo initially (matching current production callers).

{"preferred_next_label": "enough"}