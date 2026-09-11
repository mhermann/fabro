The answers covered the material questions: scope mirrors the existing `fabro-github` integration (auth + PR creation), Forgejo uses token-based auth rather than GitHub App JWT/installation tokens, and self-hosted base URLs must be first-class.

What I understand the human wants, including decisions I'm making on their behalf:

- **New `fabro-forgejo` crate** parallel to `fabro-github**: a Forgejo API client covering token auth (PAT, with base-URL config for self-hosted instances), PR creation/update, and issue comments — the same product surface `fabro-github` provides today, adapted to Forgejo's simpler token model.
- **Configuration and secrets**: a `forgejo` config section (base_url + token) loaded through the existing server-secrets startup/validation path, not a new secrets mechanism.
- **Shared abstraction kept minimal**: I'll introduce a narrow trait (e.g., PR creation) only where both forges must be interchangeable for callers, rather than a speculative full forge abstraction. The crates stay separate.
- **Sandbox clone-source contract stays GitHub-only** in this change; extending clone-based providers (Docker/Daytona) to Forgejo origins is a follow-up with its own admission/clone semantics. This is the one scope line I'm drawing myself, since it's separable and touches a documented contract.
- **Testing**: twin/scripted HTTP tests like other integration crates; no live Forgejo dependency in CI.

Remaining gaps (exact Forgejo API versions, OAuth2 app flow vs PAT-only) don't change the shape of what gets built — PAT-first with the client structured so an OAuth2 flow can be added later.

{"preferred_next_label": "enough"}