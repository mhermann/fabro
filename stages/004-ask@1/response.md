All five answers are back. Restatement for the planning stage:

## Answers and what they mean

1. **Scope — B: Core + tokens, webhooks, install, diagnostics.** Build: `fabro-forgejo` credentials, clone into Docker/Daytona sandboxes, PR create/get/merge/close (API + web UI), `[server.integrations.forgejo]` settings, sandbox token minting (`run.integrations.forgejo` → `FORGEJO_TOKEN`), webhook route, install flow (CLI + web wizard), doctor/diagnostics, docs. **No OAuth2 login, no automation-target changes** (those were scope C), and the tracker stays out regardless.

2. **Instances — A: Single instance per server.** One URL + one PAT in server settings; `owner/repo` slugs are reused scoped to that instance. No wire-format change to `GitRunTarget`/`RunTarget`, no persisted-data migration, no host-qualified identity. Non-configured hosts are rejected.

3. **Auth — A: PAT only.** One credential variant (`Authorization: token <PAT>`). No OAuth2 app machinery, no Forgejo Apps, no version gate beyond current stable Forgejo.

4. **Architecture — A: Additive parallel crate.** New `lib/components/fabro-forgejo` mirroring `fabro-github`, with explicit dispatch at boundaries. GitHub code paths, wire formats, and persisted data remain untouched — zero GitHub regression risk; duplicated shape is accepted.

5. **Webhooks — A: Inert parity.** The Forgejo webhook route verifies HMAC and logs the delivery, exactly like the GitHub route. No event-driven triggering, no new security surface.

Standing decisions from the sweep that remain in force: HTTPS-only instance URLs; accept `X-Forgejo-*`/`X-Gitea-*`/GitHub-compat webhook headers (all HMAC-SHA256); `MergeStrategy` maps directly (merge/squash/rebase), auto-merge endpoint verified against the real API during implementation; env vars `FORGEJO_URL`/`FORGEJO_TOKEN` registered in `fabro-static` env_vars + secret registry; integration default-off via `enabled` flag; tests via ported `MockHttpClient` + env-gated live tests; OpenAPI-first workflow for any spec changes; docs + changelog per repo convention.