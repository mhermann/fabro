I re-read the actual code the plan will modify — `web_search.rs` in full, `ToolSecrets` in `config.rs`, the three secret-construction sites, `check_web_search` in `diagnostics.rs`, and the register path in `tools.rs` — and evaluated each refactoring trigger against the real shape.

**What I found:**

- **The seams this change needs already exist, exactly.** `SearchBackend` is an enum with one variant per backend; dispatch is a three-line match arm per variant; each backend is one free `search_*` function plus one `format_*_results` function; shared rendering is already factored into `format_hits`/`SearchHit`; selection is one function (`from_secrets`) reading one struct (`ToolSecrets`). Adding SearXNG slots into every one of these without bending anything. The Venice backend (commit `53efde39`) landed into this same shape with no prior restructuring — that's the existence proof that the shape holds a third backend.

- **No missing seam.** What varies per backend (verb, params, auth style, response shape, quirks) is precisely what the one-function-per-backend structure isolates. Converting to a `trait SearchProvider` would be modernization, not necessity — the enum dispatch stays at three readable arms.

- **Duplication is bounded.** The send/parse boilerplate (~8 lines per backend) becomes three copies, but each backend's wire contract differs enough (Brave header auth, Venice's 400-char pre-check + 402 balance mapping + timeout, SearXNG's `format=json` + client-side truncation + 403 hint) that extracting a generic helper would couple three contracts to save a handful of lines. The genuinely shared part — rendering hits — is already shared.

- **The tuple match grows, correctly.** `from_secrets` goes from a 2-tuple to a 3-tuple match, and the arms (`(Some(brave), _, _)`, `(None, Some(venice), _)`, `(None, None, Some(url))`) are the clearest possible expression of the paid-first precedence the human chose. A priority-list abstraction would obscure it.

- **Tests reach the new behaviour directly.** The construct-backend-mutate-URL httpmock pattern works verbatim; diagnostics has vault-fixture tests. No testability gap.

- **`ToolSecrets` will hold a URL** — the name becomes marginally imprecise ("secret" vs "credential/URL"). A rename to `ToolCredentials` would touch every construction site for zero behavioral gain. The Debug impl already redacts uniformly and the vault genuinely stores it as a secret-classified value. Taste, not need.

**Named but rejected as unrelated:** deleting the past-deadline env→vault startup migration shim (`2026052501_optional_server_env_secrets_to_vault.rs`, deadline 2026-08-18) is legitimate maintainer housekeeping this change brushes against but does not need; extracting shared request boilerplate is optional polish. Neither makes this change smaller or safer.

The implementation without any refactor is a purely additive diff whose largest single-file addition (~60 production lines) lands in a module that stays under 400 production lines. Nothing is harder, more duplicated, or less testable because of the current shape. The bar for "yes" is not met.

{"preferred_next_label": "none"}