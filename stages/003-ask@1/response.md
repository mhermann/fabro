Answers received. Restatement for the planning stage:

**1. Scope → "Core run path only."** Forgejo v1 = configure credentials, run workflows from Forgejo-origin repositories in clone-based sandboxes, and open/merge/close pull requests on run completion. No webhook ingestion, no browser OAuth login, no tracker. Auto-merge (GitHub's `enable_auto_merge`) has no Forgejo equivalent → fail with a clear error, not silent no-op.

**2. Auth → "PAT only."** One vault-stored PAT for the instance, used for REST calls, git clone/push (via `Authorization: token`/URL-embedded credentials), and forwarded as the run token. No per-run scoped minting, no OAuth2 app, no JWT work. The `token_source` refresh machinery is unnecessary for a static PAT (`Static` provenance covers it).

**3. Architecture → "Sibling crate `fabro-forgejo`."** Mirror `fabro-github`'s module layout (API client, owner/repo parsing, clone credentials, PR lifecycle). Branch at call sites in server/sandbox/workflow; no refactor of the GitHub path, so zero regression risk there.

**4. Instance model → "Single instance, server config."** `[server.integrations.forgejo]` with instance `url` + vault PAT; runs whose git origin matches that host are classified as Forgejo (grows `RepositoryProvider` with a `Forgejo` variant; `GitHubRepositorySlug`/`PullRequestLink` host assumptions need generalizing for that origin). Multi-instance would be a later schema change — accepted.

**5. Web surfaces → "Fabro should be able to be configured to use an existing Forgejo instance."** Read as: pointing Fabro at an existing instance via configuration is the must-have, which both options satisfied. I'm taking the leaner reading: `fabro install forgejo` (or equivalent settings/vault config), API + regenerated TS client, and a docs page; the system-integrations status only if the existing generic endpoint makes it cheap. No web wizard step or settings-integrations card unless trivially cheap.

Decisions I made myself (unchanged by the answers): implement against the Gitea-compatible REST API under `<instance>/api/v1` (works on any Forgejo incl. Codeberg); build a twin Forgejo test double following `test/twin/github`; Docker sandbox gets Forgejo clone support first — Daytona's SDK clone is GitHub-shaped, so Daytona support uses a generic git-clone path only if it stays cheap, otherwise v1 is documented Docker-first.