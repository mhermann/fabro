## Review complete

I reviewed the change directly rather than ratifying: read the new crate in full, the webhook/PR-handler/install/clone/preflight/git-bridge diffs with surrounding code, checked the wire-format and event backward compatibility, and ran the actual test suites.

**Verified sound:**
- **Build & tests:** workspace builds clean; fabro-forgejo (31), twin-forgejo (17), fabro-sandbox (133), fabro-types (465+), fabro-server forge/webhook handlers (9), fabro-workflow (1445), fabro-api round-trips, CLI `pr_link` (4), and OpenAPI conformance (`all_spec_routes_are_routable`) all pass. An earlier CLI link failure was a transient environment linker error; clean on rerun.
- **Security:** webhook HMAC is constant-time, accepts the three real header spellings, inert by decision, auth-slot gated identically to GitHub. PAT never appears in `Debug`, is redacted via `DisplaySafeUrl`, scrubbed through the secret registry, validate-before-persist at install, URL-only in settings.toml, vault-only token. A test asserts no token in bridge env values.
- **Compatibility:** `PullRequestLink.forge` is omitted for GitHub links (byte-identical wire), serde-defaulted on read with a host+port-checked `html_url` cross-validation; events/store changes additive; GitHub code paths in `fabro-github` are untouched (additive, per the human's architecture answer); scope matches every answered question (single instance, PAT-only, inert webhooks, install flow, diagnostics, twin, docs/changelog).

**One blocking defect, confirmed empirically** (I built a scratch binary against the crate):

Port-bearing Forgejo instances — including Forgejo's own default port 3000 — do not work at all:

1. `fabro-forgejo/src/lib.rs::normalize_https_host_path` (lines 402–413) rewrites `https://host:3000/owner/repo` to `https://host/3000/owner/repo`, swallowing the port into the path. `is_forgejo_origin("https://git.example.com:3000", "https://git.example.com:3000/acme/widgets.git")` returns `false`; `parse_forgejo_owner_repo` errors. Every downstream path (clone admission, preflight, PR host selection, token gating) funnels through these two functions.
2. The bug is compounded upstream: `clone_source.rs` (`decide_clone` ~line 344, `parse_origin_owner_repo` ~line 72) and `run_manifest.rs` (`run_repository_access_check_with` ~line 850) pre-normalize origins with `fabro_github::normalize_repo_origin_url` — which contains the identical port-mangling helper — *before* the forge check. Fixing only the forgejo crate leaves sandbox cloning and preflight broken for ports.
3. No test covers it: all fixtures use port-less `git.example.com`, and the twin tests build contexts from owner/repo constants, never passing an origin URL through the normalizer. The codebase is even internally inconsistent — `PullRequestLink::from_forgejo_url` compares instance ports correctly.

**Fix (precise):**
- Make `normalize_https_host_path` port-aware in `fabro-forgejo/src/lib.rs`: when the segment between `:` and the first `/` is all ASCII digits, treat it as a port and leave the URL unchanged; only the sanitized `host:owner/repo` shape (non-numeric segment) should be rewritten.
- In `clone_source.rs::parse_origin_owner_repo`/`decide_clone` and `run_manifest.rs::run_repository_access_check_with`, run the forge check on the raw origin (`is_forgejo_origin` normalizes internally) rather than on the GitHub-normalized URL, so the GitHub normalizer's mangling can't corrupt it first. The GitHub branch stays exactly as today.
- Add tests: instance `https://git.example.com:3000` against `https://…:3000/acme/widgets.git`, `ssh://git@…:3000/…`, and a credential-bearing form; a `decide_clone` → `CloneDecision::Forge` case; a `clone_repo_layout` forge case. (Scp-style `git@host:owner/repo` without a port legitimately cannot match a port-bearing instance — no change needed.)

**Non-blocking notes:** `bun`, pinned nightly clippy, and nextest are unavailable in this environment, so web tests and clippy weren't independently rerun here (implement stage reported success; CI gates both). The twin encodes the same Gitea-API assumptions as the client, so the plan's live-test/manual E2E gate against a real instance remains the real proof of the REST shapes.

{"preferred_next_label": "fix"}