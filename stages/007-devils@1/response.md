# Attack results

I verified the plan's riskiest claims against the actual repository rather than reasoning from plausibility.

## Verified — assumptions hold

1. **Secret-create path accepts the new names.** `create_secret` (handler/secrets.rs) only rejects bootstrap secrets, malformed OAuth JSON, and invalid names. `Vault::validate_name` → `is_env_style_name` accepts `SEARXNG_URL` (alphabetic start, alphanumeric+underscore rest). `fabro secret set SEARXNG_URL ...` works as the plan's operator flow requires. No hidden allowlist.

2. **No compile breakage in files the plan doesn't touch.** I checked every `ToolSecrets { ... }` construction site: `parity_matrix.rs:227`, `profiles/mod.rs:748`, `tools.rs:1754`, `handler/llm/api.rs:3935` all use `..ToolSecrets::default()`. The only exhaustive constructions (`web_search.rs:316`, `config.rs:353`, `cli.rs:45`, `initialize.rs:316`) are inside files the plan already modifies. The plan's "no changes to profiles/parity_matrix" claim is correct.

3. **Real coupling found, assessed, not a defect:** `migrate_optional_server_env_secrets_to_store` (serve.rs:684) runs on **every server startup** and moves any `optional_vault_secrets()` name found in process env/`server.env` into the vault, stripping it from server.env. Adding our names means env-provided `SEARXNG_URL` gets auto-vaulted. This is exactly how BRAVE/VENICE behave today, and it's *beneficial* — the worker env is allowlisted (`WORKER_ENV_ALLOWLIST`), so without it, env-provided values would silently never reach runs. Consistent, not a regression. (The shim's removal deadline has passed and may be deleted by maintainers; the plan doesn't depend on it.)

4. **No snapshot/demo-test traps.** Grep confirms the remediation strings appear only in `diagnostics.rs` and `demo/mod.rs` source — no `.snap` file or demo test asserts them exactly. The diagnostics exact-string tests the plan must update (lines 1266/1287) are named. Redaction (`fabro-redact`) is pattern-based, not name-keyed.

5. **Wire patterns are proven in-file**: `.timeout()` on the request builder (Venice, web_search.rs:140), construct-backend-mutate-URL httpmock tests, `diagnostic_secret` + invalid-value connectivity-error tests — all exist as working precedent for each test the plan specifies.

## Non-blocking inaccuracies (implementer-judgment tier)

- The "sorted all-names list" in `env_vars.rs` (~line 219) is actually the `#[cfg(test)]` array for `env_var_constants_are_non_empty_and_single_tokens`, not a production constant. The described edit is still correct and worthwhile; only the description is off.
- The `web_search.rs` module doc comment (lines 1–4, "Brave is preferred... otherwise Venice") needs its precedence sentence updated — unnamed in the plan but in-file and trivial.
- The live e2e test's planned home (web_search.rs) differs from precedent (tools.rs) — both compile; web_search.rs has its own `execute` helper.
- An optional gap: no `register_core_tools`-level test that searxng-only secrets register the tool (mirroring tools.rs:1720/1750). The `from_secrets` precedence tests cover the selection logic; `register_web_search_tool` is a two-line wrapper.

None of these would make the implementation wrong or force rework — each is caught instantly by the compiler or ordinary review, well under the send-back bar.

## Scope check

`.env.example` (mirrors existing BRAVE entry, feeds the live test), changelog (repo convention), `SEARXNG_API_KEY` (vetted openly in the question stage), and the live e2e test (mirrors existing) are all justified; nothing was quietly added.

## Verification-quality check

The planned tests assert observable contracts, not tautologies: precedence outcomes per credential combination, exact wire shape (GET `/search`, `q`, `format=json`, conditional bearer), the 403 error text, truncation counts, slash normalization, and doctor selection/remediation strings. The full `fabro-agent` crate run is the regression guard that Brave/Venice behavior is untouched. Adequate to catch a wrong implementation.

The plan is sound as a basis for work.

{"preferred_next_label": "approve"}