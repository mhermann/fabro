Both files are written and validated:

**`/tmp/pr-title.txt`** — `Add Forgejo integration alongside GitHub` (40 chars, imperative, single line, no trailing period)

**`/tmp/pr-body.md`** — the full description, grounded in the actual diff (~4,600 lines across 96 files) rather than guesswork. It covers:

- **What & why**: Forgejo as a second git forge, built around a single configured instance (base URL + PAT) since Forgejo is self-hosted; GitHub paths byte-identical.
- **Per-layer changes**: the new `fabro-forgejo` client crate, origin classification (`is_forgejo_origin`) with the optional `forge` field on `PullRequestLink` (skipped in serialization for backward compatibility), config validation (`https`-only, no embedded creds, `auto_merge` rejected for Forgejo), sandbox clone/push plumbing with a redacted `SecretString` and secret-free credential helper, `PullRequestRemote` selection in the PR pipeline, server preflight/API/status routing, `fabro install forgejo`, OpenAPI + regenerated clients, web panel, and the new docs page.
- **Explicit non-goals**: webhooks, OAuth, multi-instance, forks, auto-merge, Gitea promise.
- **Reviewer focus**: the one honest gap — wire-format facts (auth header, `WIP:` drafts, `Do` merge values, no `node_id`) are fixture-verified but were not smoke-tested against a live Forgejo instance on this branch; plus origin-matching and legacy-wire compatibility.
- **Verification**: per-crate nextest suites, OpenAPI conformance test, client regeneration, web tests, and the review pipeline (four parallel reviews + deep review, none requiring follow-up fixes; earlier devil's-advocate findings shaped the design choices).