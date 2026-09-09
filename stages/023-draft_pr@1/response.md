Both PR files are written and validated.

**`/tmp/pr-title.txt`** (54 chars, imperative, no period):
> Add self-hosted SearXNG backend to the web_search tool

**`/tmp/pr-body.md`** covers:

- **What/why** — new `SearchBackend::Searxng` backend so agents get free web search via an operator-run SearXNG instance instead of per-query-paid Brave/Venice. Selection precedence: `SEARXNG_URL` → Brave key → Venice key, no inter-provider fallback.
- **Design details** — the wire contract (`GET {url}/search?format=json&pageno=1`, optional bearer, 30s timeout, client-side slicing), the config/secret split (`SEARXNG_URL` as allowlisted non-secret process-env config; `SEARXNG_API_KEY` as a vault-only secret with a negative worker-allowlist test), redacting `Debug`, the doctor probe, and the docs rollout.
- **Reviewer focus** — the intentional doctor/agent duplication of the selection rule (pinned by a test) and the secret boundaries.
- **Findings fixed along the way** — the first-round docs inaccuracy about `server.env` sourcing (fixed across 4 pages) and the devils-stage test-hermeticity pinning of pre-existing Brave/Venice doctor tests. Also lists the known minor follow-ups left as-is (the `SEARXNG_URL=/` trim-order divergence and four small test gaps) so reviewers aren't surprised.
- **Verification** — I re-ran everything myself just now: 9/9 fabro-agent searxng tests, 8/8 fabro-server web-search/allowlist tests, 21/21 fabro-workflow initialize tests, and `fmt --check` clean; plus the four independent review passes that ran the broader suites.