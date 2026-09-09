I attacked the plan against the repository as it actually is. Verdict and findings:

## Verified sound (the load-bearing claims)

- **Worker plumbing**: confirmed `lib/apps/fabro-cli/src/commands/run/runner.rs:268` (`load_worker_vault`) loads the vault from the shared storage dir, so `vault.get(SEARXNG_API_KEY)` works in the worker; and `spawn_env.rs` copies the *server's* process env through the allowlist, so `SEARXNG_URL` crosses `env_clear()`. The dual-path design (URL=env, token=vault) is real, not aspirational.
- **Single secret construction site**: `initialize.rs:359` computes `ToolSecrets` once; line 265's `tool_secrets_for_api` is a clone. No hidden second path.
- **Struct-literal blast radius**: enumerated all `ToolSecrets {` sites (`cli.rs:45`, `initialize.rs:316`, `config.rs:353`, `web_search.rs:316`, `tools.rs:1753`, `api.rs:3932`, `parity_matrix.rs:227`). Compiler catches each; step 11 covers them.
- **Clippy**: `std::env::var` is in `clippy.toml` `disallowed-methods`, so the `#[expect]` in `initialize.rs` is *definitely* required, not conditional — trivial.
- **No UI coupling**: `apps/fabro-web` only maps the tool name to a label; doctor results render generically. Correctly excluded.
- **Snapshots/demo**: profile prompt snapshots inject keys directly (unaffected); the demo remediation string is not test-pinned.
- **`fabro secret set SEARXNG_API_KEY`** will be accepted (no restrictive name allowlist in `secret/set.rs`).

## Defects found — corrections the implementer must absorb (not redo-inducing)

1. **The plan makes a false claim about existing diagnostics tests.** `TestAppStateBuilder` defaults `env_lookup` to the **real process env** (`test_support.rs:655`). The existing precedence tests (`check_web_search_prefers_brave_when_both_vault_keys_exist`, the venice/env-backed ones) do not override it. Once `check_web_search` consults `SEARXNG_URL` first, any ambient `SEARXNG_URL` in a developer shell flips those tests to the searxng branch. The plan says they "keep passing with only remediation-text edits" — wrong. Fix while implementing: pin `env_lookup` (returning `None`) in those tests, and have the new searxng tests set/absent the URL explicitly. Important trap to avoid: the failure mode must be fixed by pinning env in tests, **not** by reordering `check_web_search` — reordering would make doctor report a different backend than runs actually use, silently inverting the human's "local wins" decision.
2. **Four doc files missing from the list.** `venice-search.mdx:20,49` states "Fabro prefers direct Brave Search whenever `BRAVE_SEARCH_API_KEY` is also present" — false after this change; `brave-search.mdx` frames Brave as the automatic default; `deploy-railway.mdx:44` and `self-host-docker.mdx:111` enumerate vault secrets ("Brave Search, Venice Search") where `SEARXNG_API_KEY` belongs. Add them to phase 5; without them the docs self-contradict.

Two non-blocking placement notes: put the empty/whitespace URL filtering in `from_secrets` (single choke point) rather than only `initialize.rs`, so the standalone-CLI path can't register a broken backend on `SEARXNG_URL=" "`; and be aware that adding `SEARXNG_API_KEY` to `OPTIONAL_VAULT_SECRETS` also enrolls it in the still-wired env→vault startup migration (`serve.rs:683`) — identical to BRAVE/VENICE behavior, benign, no action.

Both corrections are minutes of work with unambiguous fixes; the architecture, file map, precedence wiring, and verification strategy all check out against the actual code. The plan is a sound basis for implementation.

{"preferred_next_label": "approve"}