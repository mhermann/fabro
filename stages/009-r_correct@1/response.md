Review complete. I read the full diff (`abb07abf..HEAD`, 21 files), traced every consumer of the changed types, and ran the relevant test suites in this checkout — all pass (fabro-agent web_search/config, fabro-server diagnostics, fabro-static, fabro-workflow initialize).

## What I verified as correct

- **Selection ladder** (`web_search.rs:42-53`): SearXNG > Brave > Venice matches `from_secrets`, the doctor probe order (`diagnostics.rs:762-786`), and every doc claim. All backend consumers (`tools.rs:94`, `claude5.rs:47`) route through `from_secrets` — no missed exhaustive match sites.
- **Request/parse logic**: `limit` clamped to 1..=20 (same as Venice; Brave's `min`-only is pre-existing), client-side `.take(limit)` truncation, `SearchHit` field mapping (`content`/`publishedDate` match SearXNG's real JSON schema), trailing-slash trim, 403→`search.formats` hint. httpmock tests pin method, path, and query params.
- **Plumbing**: both production `ToolSecrets` construction sites (`cli.rs:44`, `initialize.rs:317`) read the new field; registry/env-var additions make `fabro secret set/rm SEARXNG_URL` work; `secret rm` exists as documented; Debug redaction covers the URL.
- **Doctor semantics**: vault-only resolution (`vault_secret` → `Vault::get`, no env fallback) is consistent with the existing Brave/Venice checks and with runtime (vault for server runs, env for CLI) — the new `check_web_search_ignores_env_backed_searxng_url` test pins this deliberately.
- **Compose/settings**: searxng on the same default network as `fabro` (so the documented `http://searxng:8080` URL resolves from the server container), JSON format enabled (required, since default SearXNG 403s), `docs.json` parses.

## Finding — empty `SEARXNG_URL` shadows working paid backends with a cryptic failure

**Files:** `web_search.rs:48` (`(Some(base_url), _, _) => Some(Self::searxng(base_url))`), enabled by unfiltered reads at `cli.rs:46` and `initialize.rs:317`.

**What's wrong:** `Some` is matched unconditionally, and `SearchBackend::searxng()` (line 58) trims but never rejects an empty result. An empty-but-present value therefore *selects* SearXNG — which by the new precedence **disables a configured, previously-working `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY`** — and every `web_search` call then fails with `HTTP request failed: builder error: relative URL without a base` (`format!("{base_url}/search")` on `""` → `Url::parse` failure at `send()`).

**Concrete triggers:**
1. The diff adds `SEARXNG_URL=` (empty) to `.env.example`. A user who sources `.env` (the repo's documented `set -a && source .env` flow) and runs the standalone agent CLI gets `searxng_url: Some("")` from `std::env::var(...).ok()` → SearXNG selected over their real Brave key.
2. `fabro secret set SEARXNG_URL ""` stores an empty vault value: the emptiness guard in `resolve_value` (`fabro-cli/src/commands/secret/set.rs:29-31`) is bypassed for the explicit positional — it returns `Ok(value)` before the checks that cover the stdin/interactive paths. Server runs then behave as in (1); `fabro doctor` shows only `searxng: connectivity error`.

**Honest context:** the empty-env-selects-backend pattern pre-exists for the two keys, so this is not a novel class of bug — but the diff both adds a new empty-inviting line for the variable now at the top of the precedence ladder and worsens the failure mode (builder error instead of an upstream 401). One-line fix at the root: treat empty-after-trim as unconfigured in `from_secrets` (e.g. skip `Some(s) if s.trim().is_empty()`), so selection falls through to Brave/Venice.

## Non-findings I checked and rejected

- No per-request timeout on the SearXNG call — matches Brave's existing behavior; Venice's 1-minute timeout is the outlier, not the rule.
- System-proxy policy applies to the localhost call — same client (`fabro_http::http_client()`) is used by doctor and runtime for all three backends, so no divergence; matches existing convention.
- No cancellation-token check in the executor — pre-existing for all backends.
- Historical changelog (`2026-08-21.mdx`) still describes the old ladder — dated records, correctly left alone.