All three answers are in. Restating for the record — this is what the planning stage should build from:

## Answers and what they mean

1. **Scope — "b) Core + install/ops surface."** The change includes the run-path core (sandbox clone from Forgejo, run-driven PR create/view/link/merge/close, PAT credentials in settings/vault, docs + changelog) **plus** the operational surface: `fabro install` support (interactive and non-interactive flags), browser install-wizard routes in the server, system-integrations status reporting, diagnostics, and doctor checks. It does **not** include Forgejo OAuth login for the web UI, inbound webhooks, or Forgejo automations/server-submitted run targets — GitHub's `server.auth`, webhook stack, and `GitRunTarget`/admission model stay untouched. (Install wizard collects instance URL + PAT per the standing PAT-only-v1 decision.)

2. **Instances — "a) Single configured instance."** One Forgejo/Gitea instance per server: `[server.integrations.forgejo]`-style single `url` + one token in the vault; run/PR origins must match that host. Persisted PR links still carry their host so widening to multiple instances later is non-breaking.

3. **Gitea — "b) Forgejo + Gitea."** Same code, dual-branded "Forgejo/Gitea" across docs, settings naming, error messages, and validation; Gitea instances (gitea.com and self-hosted CE) are first-class and covered by the testing matrix.

Combined with the standing decisions (new `fabro-forgejo` crate mirroring `fabro-github`, host-aware `PullRequestLink`, PAT-only v1, fail-loud error for `auto_merge` on Forgejo, `WIP:`-title draft mapping, three shared merge strategies mapped with clear errors, system-trust TLS, `FORGEJO_URL`/`FORGEJO_TOKEN` env + vault pipeline, mock-HttpClient tests with env-gated live tests, `IntegrationProvider::Forgejo` status, docs page + changelog), the scope is now fully determined.