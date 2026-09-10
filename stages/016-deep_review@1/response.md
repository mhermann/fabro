My review is complete. Summary of what I verified independently:

**Change vs. goal and human decisions.** The goal was a cheaper-than-Brave/Venice web search source running locally; the human chose self-hosted SearXNG, operator-managed. The diff delivers exactly that: a `Searxng` backend variant behind `SEARXNG_URL` (vault for server runs, env for the standalone CLI — mirroring the existing two keys), no container-lifecycle code, no provider enum resurrected, no OpenAPI surface touched. Precedence `searxng > brave > venice` is implemented at both selection layers, and the no-fallback invariant is preserved and re-documented.

**Whole-repo correctness.** All `ToolSecrets` literals already used `..ToolSecrets::default()` on main, so the untouched `tools.rs`/`profiles/mod.rs`/`parity_matrix.rs`/`api.rs` files are correct, not missed (I checked each literal). The formatter truncates client-side with the instance's ranking preserved; `Debug` redaction covers the new field (tested); query strings go through reqwest's parameter encoding, so no injection surface; the URL is operator-vault config, same trust class as existing `base_url` settings.

**Tests are discriminating.** The diagnostics precedence test uses an intentionally invalid Brave key as a canary — if SearXNG weren't checked first, the check would fail. The httpmock tests assert exact path, `q`/`format=json` params, `publishedDate` rendering, truncation, and both error mappings; the URL-validation test uses `assert_calls(0)` to prove pre-HTTP rejection. Wrong code fails these.

**Full tooling sweep, all green:** 658/658 `fabro-agent` + `fabro-static`, 7/7 web-search diagnostics tests, 1444/1444 `fabro-workflow`, `cargo +nightly-2026-04-14 fmt --check --all` clean, `clippy --workspace --all-targets -- -D warnings` clean (the `#[expect]` on `Url::parse` is documented and would itself fail if unnecessary), `docs.json` valid JSON with **both** the integrations nav entry and the changelog nav entry — the one defect I flagged at plan review was addressed.

**Minor, non-blocking:** `check_searxng_search` probes without validating the URL scheme first, so a malformed vault value surfaces as a "connectivity error" warning rather than a config-specific message — still a warning with actionable remediation, and the agent-side backend does validate strictly.

{"preferred_next_label": "approve"}