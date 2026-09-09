# Review: Forgejo integration (main...HEAD, ~4,600 lines)

I read the full diff plus surrounding context in the sandbox, workflow, server, and CLI layers. The overall structure is sound: provider selection happens once (origin → remote), the GitHub paths are preserved byte-for-byte for existing data, and the wire-format fixtures are internally consistent. I found **four correctness defects** in the new call-site wiring, all with concrete triggers.

---

## Finding 1 — Forgejo PR creation via the API is blocked on servers without GitHub credentials (high)

**File:** `lib/apps/fabro-server/src/server/handler/pull_requests.rs:409`

`create_run_pull_request` now correctly lets a Forgejo origin through `RunPrInputs::extract` (line 404, via the new `forgejo_base_url` param) — but the very next statement is unconditional:

```rust
if let Err(err) = load_server_github_credentials(state.as_ref()).await {
    return err.into_response();
}
```

`load_server_github_credentials` returns **503 "GitHub integration unavailable on server."** when no GitHub token/app is configured. The background supervisor that actually creates the PR (`pull_request_supervisor.rs:186-198`) was correctly changed to load GitHub credentials *only* for non-Forgejo origins — so the HTTP handler contradicts its own worker.

**Trigger:** Forgejo-only deployment (no GitHub integration configured), a completed run with a Forgejo origin, `POST /api/v1/runs/{id}/pull_request` (what `fabro pr create` calls) → 503 before the creation is ever enqueued, even though the supervisor would create the PR through Forgejo without touching GitHub at all.

## Finding 2 — `run.scm.provider = "forgejo"` fails when the instance is configured in settings only, contradicting its own error message and the docs (high)

**File:** `lib/components/fabro-manifest/src/lib.rs:438-451`

```rust
let Some(base_url) = fabro_forgejo::forgejo_base_url(None) else {
    anyhow::bail!("run.scm.provider = \"forgejo\" requires a configured Forgejo instance URL; \
         set server.integrations.forgejo.url or FORGEJO_URL");
};
```

`fabro_forgejo::forgejo_base_url(None)` reads **only `std::env::var("FORGEJO_URL")`** (`fabro-forgejo/src/lib.rs:70-84`). Settings are structurally unreachable here — `ManifestBuildInput` carries no Forgejo URL, and nothing exports `FORGEJO_URL` from settings to the process env (I searched; the only settings-aware resolvers are `AppState::forgejo_base_url` and the CLI's `shared/forgejo.rs`). Meanwhile:

- `fabro install forgejo` writes **only** `[server.integrations.forgejo]` to settings.toml plus the vault token — `server_env_set: Vec::new()` (`commands/install.rs`, `fabro-install/src/lib.rs`), no env var;
- the docs promise exactly the settings path: "The instance URL resolves from `server.integrations.forgejo.url` (or the `FORGEJO_URL` environment variable as a fallback); … `fabro run` builds the origin from the configured instance" (`docs/public/integrations/forgejo.mdx`);
- `GET /system/integrations` will report Forgejo `configured` in this state (`system.rs` checks the settings URL), and the CLI's own credential builder for the *same run* is settings-aware — only the origin build isn't.

**Trigger:** operator runs `fabro install forgejo` (settings-only), then `fabro run` on a workflow with `[run.scm] provider = "forgejo"` → manifest build fails, telling them to set `server.integrations.forgejo.url`, which they already did. Same failure for `fabro preflight`/`validate`/`graph`.

## Finding 3 — Mixed URL normalizers break Forgejo origins that contain an explicit port (medium)

Two Forgejo-classifying call sites normalize the origin with **`fabro_github::normalize_repo_origin_url`**, whose `normalize_https_host_path` (fabro-github/src/lib.rs:1100-1108) rewrites `https://host:8443/path` into `https://host/8443/path` (port digits become a path segment — the `!path.starts_with('/')` guard doesn't stop `8443/...`). The Forgejo sandbox path uses the port-preserving `fabro_forgejo::normalize_repo_origin_url`, so the two layers disagree:

- `lib/apps/fabro-server/src/run_manifest.rs:895` (`run_forgejo_origin_check`) and `:828` (`run_repository_access_check_with` → `check_forgejo_remote_ref`)
- `lib/apps/fabro-server/src/server/handler/pull_requests.rs:328` (`RunPrInputs::extract`, whose `normalized_origin` the supervisor then feeds to `open_pull_request`)

The mangled URL still *classifies* as Forgejo — `is_forgejo_origin` compares hosts only and ignores the base URL's port — and then fails `parse_forgejo_owner_repo`'s prefix match (`{base}/` no longer a prefix).

**Trigger:** instance at `https://git.example.com:8443`, run origin `https://git.example.com:8443/acme/widgets` (what the manifest builder itself produces for such a base URL) or SSH remote `ssh://git@git.example.com:2222/acme/widgets.git`. The sandbox clones fine (forgejo normalizer), but preflight "Repository Access" fails, and an async `POST …/pull_request` is accepted (202) then ends `Failed` with "Not a URL on the configured Forgejo instance". Self-hosted instances on non-default ports are the norm, so this is not exotic.

## Finding 4 — `fabro repo init` classifies Forgejo remotes from env only, so the settings-only install never reports Forgejo access (low)

**File:** `lib/apps/fabro-cli/src/commands/repo/init.rs:205`

```rust
let forgejo_base = fabro_forgejo::forgejo_base_url(None);
```

Again env-only, while the rest of the CLI resolves settings-first via `shared/forgejo.rs::forgejo_base_url_from_server_settings`. After `fabro install forgejo` (settings-only, per Finding 2), a `git@forgejo.example.com:acme/widgets` remote is classified as non-forgejo, `parse_github_owner_repo` fails, and the function silently skips — the check endpoint (`/repos/forgejo/...`) added by this change is never consulted for a properly installed instance.

---

## Non-bug observations

- **`FORGEJO_TOKEN` is injected into every sandbox env whenever Forgejo creds exist**, including GitHub-origin runs (`initialize.rs:106-107`; only the git credential helper is origin-gated). Flagged as deliberate in the implement-stage summary; it widens secret exposure beyond Forgejo runs, worth a follow-up decision rather than a review block.
- **`operations/fork.rs:127-137`** now rejects *any* non-`github.com` origin with "fork is not supported for Forgejo runs" — fail-closed correct (a plain-git origin fork would have failed at clone anyway), but the message misnames the provider for non-Forgejo origins.
- `run_forgejo_origin_check`'s `Ok(None) => false` branch (run_manifest.rs:928) pushes no check row; unreachable in practice since `forgejo_credentials()` returns `Ok(None)` only when no base URL is configured, which is guarded above. Defensive dead code, no action needed.
- The `.install.rs.pending-snap` file in the CLI test dir is an untracked leftover from the implementer's run and matches the committed snapshot; not part of the diff.

## What I checked and found correct

`is_forgejo_origin` host/subpath matching (including the `.evil.test` prefix case and trailing slashes); the `PullRequestLink` custom serde (legacy GitHub JSON stays byte-identical; html_url cross-validation), with all construction sites passing trimmed base URLs so the trailing-slash round-trip edge can't fire; `from_stored_pr_url` base-path reconstruction; the demo-mirror route; the `GIT_CONFIG_COUNT` arithmetic when both bridges would apply (github bridge re-reads the count after the forgejo entry, no collision); sandbox `decide_clone`/layout dispatch for both providers; the `WIP:`/`Do`/`commit.id`/201-on-edit fixtures; `RepositoryProvider`/`IntegrationProvider` enum parity with the OpenAPI spec and generated clients; and the auto-merge rejection for Forgejo at both config resolve and the `enable_auto_merge_if_requested` guard.

The four findings above are the ones I'd fix before merge; Findings 1 and 2 break the primary Forgejo-only flow on the exact setup this change ships (`fabro install forgejo`).