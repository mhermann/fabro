The answers covered the scope questions that mattered: which parts of the GitHub integration to mirror (auth, PR creation/upsert, comments, and clone-source support in the sandbox layer), and that self-hosted instances plus Codeberg both need to work via a configurable instance URL.

What I now understand the human wants:

- **A Forgejo integration parallel to `fabro-github`**: token-based auth, PR creation/update, and comment posting, as a sibling crate/module using Forgejo's API rather than forking the GitHub client wholesale.
- **Sandbox clone-source support**: the Docker/Daytona providers' GitHub-only origin contract extends to recognize Forgejo URLs (host + owner/repo) so run manifests with a Forgejo origin clone correctly, with the same branch/exact-commit semantics and no ancestry fallback.
- **Server/config surface**: instance base URL + token as first-class settings, so self-hosted and Codeberg both work without code changes.

Decisions I'm making on their behalf:

- **PAT-only auth in the first cut.** Forgejo's OAuth-app flow has no clean equivalent of the GitHub App JWT/installation-token machinery, so I'm not building that bridge now; token auth covers both hosted and self-hosted.
- **Reuse the GitHub-compatible API shape where it genuinely matches** (repos, pulls, issues/comments), but keep the transport/config types distinct so the existing GitHub paths don't grow a `base_url` special case — consistent with the "no accidental duplicate types" rule in the codebase.
- No webhook/trigger work in this pass, since that wasn't part of the GitHub feature set being mirrored.

That's enough to plan against the real codebase structure.

{"preferred_next_label": "enough"}