## Final review

Last round I sent this back with one blocking defect: port-bearing Forgejo instances — including Forgejo's own default port 3000 — were entirely broken because `normalize_https_host_path` swallowed the port into the path, compounded by the GitHub normalizer running before forge dispatch in `clone_source.rs` and `run_manifest.rs`.

**The fix is correct, complete, and proven — not just present:**

1. **Normalizer** (`fabro-forgejo/src/lib.rs`): the rewrite now skips URLs whose first path segment after `:` is all digits (a port); only the sanitized scp-like `host:owner/repo` shape is rewritten. I re-ran my independent scratch binary: `:3000` instances now match and parse port-bearing HTTPS, credential-bearing, and `ssh://` origins; portless instances correctly *reject* `:3000` origins (a different server); the port-incapable scp form fails with a clear instance-naming error. The one theoretical miss (all-numeric repo *owner* in the sanitized shape) is unreachable — Forgejo/Gitea forbid purely-numeric usernames — and fails cleanly anyway.

2. **Dispatch order** (`clone_source.rs`, `run_manifest.rs`): the forge check now reads the raw origin (`is_forgejo_origin` normalizes internally), so the GitHub normalizer can never corrupt a port first; each branch stores its own canonical form; `clean_clone_origin_for_record` preserves ports in persisted records; the GitHub branch is byte-identical to before.

3. **Tests fail if the code regresses**: crate unit tests for ports, sandbox `decide_clone`/layout/record port tests, a run_manifest regression test that uses a closed loopback port to prove the forge branch runs *and* the GitHub probe never fires, and a twin end-to-end test noting every twin URL is port-bearing — closing the exact coverage hole that let the bug ship.

**Verification:** fabro-forgejo 35 ✓, twin-forgejo 18 ✓, fabro-sandbox 133 ✓, fabro-server forge + port regression 10 ✓, workspace build clean, plus my out-of-tree behavioral matrix. Workflow (1445) and conformance suites passed last round on code this fix round didn't touch.

**Whole-change intent check:** delivered scope B exactly as the human answered — single instance per server, PAT-only, additive crate (`fabro-github` untouched), inert HMAC-verified webhooks, full PR lifecycle across CLI/server/web, install flow, diagnostics, twin, docs + changelog; `PullRequestLink.forge` is additive and omitted for GitHub links.

**Non-blocking notes on record:** `bun`, pinned nightly clippy, and nextest are unavailable in this sandbox, so web tests/clippy weren't independently rerun here (implement stages reported success; CI gates both). The twin shares the client's Gitea-API assumptions, so the documented live-test/manual pass against a real instance remains the definitive REST-shape proof — worth mentioning in the PR description.

This is now worth reviewer time.

{"preferred_next_label": "approve"}