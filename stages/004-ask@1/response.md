All four answers are back. Restating them for the planning stage:

## Answers and what they mean

**1. Capability scope → Full run-target parity.** Forgejo repos become first-class run targets: clone-based sandbox execution (Docker/Daytona), checkpoint push, auto-PR/auto-merge, and sandbox token injection. This *requires* the wire-type work: a forge/host field on `GitRunTarget` and `PullRequestLink` (additive serde-default fields only), lifting the sandbox layer's explicit "GitHub origins only" contract in `clone_source.rs`, and regenerating OpenAPI → progenitor Rust → TS client. Webhooks were part of the old "tier C" bundle that got merged out, so they are **not** included; the GitHub-Projects-style tracker remains **excluded** (no Forgejo analogue — GitHub's is GraphQL-only).

**2. Instance topology → Single configured instance.** One base URL + one PAT declared in settings (`[server.integrations.forgejo]`-style), matching the Slack/Daytona single-credential pattern. No host-keyed credential store, no per-origin matching, no Codeberg hardcoding. Any Forgejo origin a run points at must resolve against that one configured instance; other hosts are rejected rather than credential-matched.

**3. Auth model → PAT only.** One static scoped token stored in the vault. No OAuth2 app, no `ServerAuthMethod`/`AuthMethod::Forgejo`, no install-wizard OAuth flow, no web-login surface — mirroring the GitHub `token`-strategy precedent. Since Forgejo has no installation-token equivalent, none of the token-minting/refresh machinery is ported; the sandbox token is injected as a static PAT.

**4. Code architecture → Parallel `fabro-forgejo` crate.** A contained new component crate mirroring the needed `fabro-github` surface (token-only credentials, REST client, URL math, credential-helper config). No refactor of `fabro-github`; every existing GitHub path stays byte-for-byte untouched; intentional duplication is accepted.

## Standing decisions carried forward

No data/DB migrations (existing persisted data is all github-shaped; additive fields keep it valid); wire changes are additive-with-defaults only; `FORGEJO_*` secrets registered in `fabro-static` EnvVars + secret/redaction registries, vault-only in the server process with no env fallbacks; Forgejo PR URLs use `/pulls/N`; token-scheme auth header; `test/twin/forgejo` harness mirroring `test/twin/github` for tests (twin-only, no live tests); docs page + `docs.json` nav + `IntegrationProvider::Forgejo` status + changelog entry; client built against the Gitea-compatible API but branded Forgejo-only.