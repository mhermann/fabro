Both files are written under `/tmp` only:

**`/tmp/pr-title.txt`** — `Add self-hosted SearXNG as the preferred web_search backend` (59 chars, imperative, no period)

**`/tmp/pr-body.md`** — the full description, covering:

- **What and why**: adds a free, self-hosted SearXNG backend to the paid-only `web_search` tool (Brave/Venice), selected via `SEARXNG_URL`. Fabro connects to an operator-run instance; it doesn't manage it.
- **How it works**: selection order SearXNG → Brave → Venice (free wins when configured; zero change for users who don't set the URL), the `GET /search?q=…&format=json` wire contract with 30s timeout, pre-HTTP URL/scheme validation, the 403-specific "whitelist the json format" error hint, client-side `max_results` truncation (SearXNG has no count param), redacted `ToolSecrets` plumbing, vault-vs-CLI env split, the deliberate omission from the secret registry (URL, not a credential — no migration entry), and the `fabro doctor` probe updates.
- **Reviewer focus areas**: precedence over paid keys, the 403 message coupling, truncation strategy, timeout choice, and the `disallowed_types` expect on URL parsing.
- **Verification**: 30 fabro-agent web_search tests (wire contract, precedence, truncation, error mapping, schema identity), 7 fabro-server diagnostics tests, redaction test, env-var constant tests — all passing; fmt and clippy clean. I re-ran these suites to confirm before writing the description.
- **Review findings**: the parallel correctness/security/tests/simplicity reviews and deep review passed with no code changes required (the post-implement stage commits contain no file changes), with security confirming vault-only trust, no URL logging, and unchanged error surface.