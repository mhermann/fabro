Both scans are clean. The secret hits are env var *names* read via `std::env::var` in live-gated e2e tests (no literals), the `credentials = [...]` line is unchanged pre-existing context, and the single non-ASCII byte is an em-dash in docs prose. No dependency files, auth code, or path handling were touched (diff stat confirms: catalog TOML, catalog Rust code/tests, e2e tests, docs only).

## Security review: no security problems

I reviewed the full diff (`origin/main...HEAD`, 10 files) for injection, committed secrets, unvalidated input reaching sinks, authorization gaps, unsafe deserialization, path traversal, and problematic dependencies. Findings:

**Nothing found.** Specifics on what I checked:

- **Secrets/credentials** — The two new e2e tests (`integration.rs`) read `ZAI_API_KEY` / `OPENROUTER_API_KEY` from the environment at test runtime, gated behind `#[e2e_test(live(...))]`, which is exactly the pattern of the pre-existing GLM 5.2 tests in the same file. No key material, tokens, or fake credentials appear in the diff or its snapshots.
- **Egress endpoints** — The tests hardcode `https://api.z.ai/api/coding/paas/v4` and `https://openrouter.ai/api/v1`: HTTPS, official vendor domains, identical to the incumbent tests. No attacker-controlled or plaintext destinations.
- **Injection** — All new data is static TOML catalog entries (model ids, aliases, limits, costs) parsed by the existing serde catalog loader; no string is interpolated into a shell command, URL, or query. The Rust additions are tests with fixed literals.
- **Untrusted input / deserialization** — No new input paths. Catalog TOML is compile-time embedded data; the test-side `tomc::from_str` parses a constant string. No `unsafe`, no dynamic deserialization of external data.
- **Authorization** — Untouched; the diff only adds model rows and reassigns provider defaults/aliases.
- **Path traversal** — No filesystem paths introduced anywhere in the diff.
- **Dependencies** — No `Cargo.toml`, lockfile, or package manifests changed, so nothing new to supply-chain.

One behavioral note that is *not* a security issue but worth awareness: the default/alias remap means existing users' unpinned `glm`/`glm5` selectors silently move from GLM 5.2 to GLM 5.3 (costs are identical at $1.40/$4.40, so no billing surprise). That's an intentional product change following repo precedent, documented in the changelog.