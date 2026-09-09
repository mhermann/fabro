Review complete. I've read the full diff (`main...HEAD`, 24 files) plus the surrounding context — the shared HTTP client facade, the diagnostics probe helpers, and the secret registry wiring.

## Verdict: no security problems found

The change adds a SearXNG backend to `web_search`: `SEARXNG_URL` (process env, non-secret) selects it over Brave/Venice; optional `SEARXNG_API_KEY` lives in the vault and is forwarded as a bearer token. The secret-handling design is careful, and I verified each boundary rather than trusting the comments:

**Verified clean:**

- **Secret/config split is correct.** Only `SEARXNG_API_KEY` was added to `OPTIONAL_VAULT_SECRETS` (`secret_registry.rs`); `SEARXNG_URL` is deliberately not registered as a secret. The one-line strategy-doc update matches.
- **Worker env boundary preserved.** `spawn_env.rs` allowlists `SEARXNG_URL` to survive the worker's `env_clear()`, and explicitly excludes `SEARXNG_API_KEY` — with a regression test asserting the key does *not* cross (`assert!(!actual.contains_key("SEARXNG_API_KEY"))`). The key reaches workers via the vault only.
- **No credential leakage in logs or errors.** `ToolSecrets`' custom `Debug` impl exposes only `searxng_url_configured`/`searxng_key_configured` booleans, and a test asserts neither the host nor key value appears. `searxng_status_error` and `match_web_search_probe` emit status codes and error chains only — no key, response body, or header echo. No `Serialize` derive on `ToolSecrets`.
- **No injection.** The agent-controlled query goes through reqwest's `.query()`, which percent-encodes; the doctor probe uses a static query with the trailing slash trimmed off the base URL. The URL is never interpolated into shell or paths.
- **No attacker-reachable SSRF.** The probe/search target comes from `std::env::var` — server process env only, which neither sandboxed workflows nor API callers can write. Both consumers (agent tool, doctor) are pre-existing surfaces; only the target changed, and only the operator can set it.
- **Nothing committed.** `.env.example` has empty placeholders; test credentials are obviously fake (`"leak"` is used in a negative assertion); docs use `<token>`. No `unsafe`, no new dependencies, no router/auth changes (the `demo/mod.rs` change is a fixture string only).

**Minor notes, not defects:** the response body is parsed without a size cap, but that's the exact pattern Brave/Venice already use and requires control of the operator-run instance to abuse; redirect behavior is the shared reqwest default (sensitive headers stripped on cross-host redirects), unchanged by this diff; enrolling `SEARXNG_API_KEY` in the startup env→vault migration matches Brave/Venice behavior exactly. Search-result content flowing into agent context is prompt injection by nature of any web search tool — inherent to the feature, not introduced here.