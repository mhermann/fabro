Review complete. Here is my correctness assessment of the Forgejo integration (127 files, ~8.4k insertions).

## What I verified as correct

- **Webhook HMAC** (`forgejo_webhooks.rs`): constant-time `verify_slice`, fail-closed on missing/invalid signature, fixed header precedence all verifying against the same secret, route mounted only with a secret, auth-slot invalidation on failure.
- **Credential placement**: Forge clone never mints a GitHub token (`build_token_source` returns `None` for non-github.com origins); the Forge decision and `self.forgejo` derive from the same immutable field, so the GitHub-token fallback in `docker.rs:970` can't mis-fire; PAT never reaches logs, settings.toml, or the install session.
- **Token injection fails closed** (`initialize.rs::resolve_forgejo_token`): requested-without-instance or off-instance-origin both hard-error before any stage runs.
- **Publish/supervisor host dispatch**, `PullRequestLink` serde (byte-identical wire format for existing records), `from_forgejo_url` host+effective-port matching, and `GIT_CONFIG_COUNT` layering between the Forgejo and GitHub bridges.

## Findings

**1. Origin normalization corrupts URLs with an explicit port — breaks every custom-port instance** (high; verified by executing the verbatim function bodies)
`lib/components/fabro-forgejo/src/lib.rs:402-413` (`normalize_https_host_path`, used by `normalize_forgejo_origin_url:419`). `rest.split_once(':')` treats `host:port/path` as `host:path`: `https://git.example.com:3000/owner/repo` normalizes to `https://git.example.com/3000/owner/repo`; `ssh://git@git.example.com:2222/owner/repo.git` → `https://git.example.com/2222/owner/repo`. `ForgejoInstance::parse` itself is port-aware (host includes the port), so the two disagree. Trigger: self-hosted instance on any non-443 port — Gitea's **default port is 3000**, so this is the most common deployment shape. Consequences: `is_forgejo_origin` returns false (→ `FORGEJO_TOKEN` not injected, clone classified as unsupported/GitHub), `parse_forgejo_owner_repo` fails, `pr link` classification and preflight access check fail. This is a faithful port of the GitHub helper, where it's latent (github.com never has ports) — here it's live. Tests miss it because client-op tests take `owner/repo` directly and never push a port-bearing origin through the normalizer.

**2. Server-side PR creation for Forgejo runs is unreachable — `RunPrInputs::extract` gates it to github.com**
`lib/apps/fabro-server/src/server/handler/pull_requests.rs:433` unconditionally calls `parse_github_owner_repo_from_url`, which returns 400 `unsupported_host` for any non-github.com host (lines 49-57). Both consumers — the supervisor (`pull_request_supervisor.rs:173`) and the create endpoint (`pull_requests.rs:500`) — hit this gate **before** their Forgejo host-selection arms (`pull_request_supervisor.rs:189-199`). Trigger: `POST /runs/{id}/pull_request` on a Forgejo-linked run → 400 "support github.com only". Either dead code that should be deleted or a missed dispatch: Forgejo runs can only get a PR via the publish stage, contradicting the "PR create (API + web UI)" scope.

**3. Preflight's aggregate verdict contradicts its own Forgejo token check**
`lib/apps/fabro-server/src/run_manifest.rs:519-560`: the new "Forgejo Token" check can report `Error/unavailable`, but `checks_ok` (line 559) doesn't include it — unlike the GitHub token sibling. Trigger: run with `token = true` and an off-instance origin → preflight reports `preflight_ok = true`, then the run hard-fails at initialize (`Error::Precondition`). The comment says "informational", so possibly deliberate, but it mispredicts runnability.

**4. Local CLI runs hard-fail on `FORGEJO_URL` without a token, even for unrelated runs**
`lib/apps/fabro-cli/src/commands/run/runner.rs:145-150`: `build_forgejo_config(None, &vault_guard)?` propagates "FORGEJO_TOKEN not configured" for **every** local run. Trigger: `FORGEJO_URL=https://git.example.com fabro run <github-only-workflow>` with no token → run refuses to start. Asymmetric with `maybe_build_github_credentials` (optional) and with the server-side resolver, which degrades to `None`.

**5. `forgejo_integration_status` counts a blank URL as configured**
`lib/apps/fabro-server/src/server/handler/system.rs:144`: `settings.url.is_none()` vs. the trim-and-filter used by `forgejo_config` (server.rs:1639-1646) and everywhere else. Trigger: `url = " "` with `enabled = true` → `GET /system/integrations` reports `Configured` while the integration is effectively inert.

**6. Reinstalling without the Forgejo step leaves a stale `FORGEJO_TOKEN` in the vault**
`lib/apps/fabro-server/src/install.rs` (`post_install_finish`): GitHub keys are pushed to `vault_removals` on the skip path (lines 1731, 1784); `FORGEJO_TOKEN` never is, while settings.toml is rewritten without the section. Trigger: install with Forgejo, reinstall skipping it → orphaned secret that silently reactivates if the section is re-added by hand.

**7. Message/label-only defects** (behavior correct, text wrong)
- `clone_source.rs:90`: path-component errors always say "GitHub …" even for Forge origins.
- `docker.rs:812-815` / `daytona/mod.rs:1738-1743`: a Forge clone failing on a bad PAT gets "configure a GitHub App" remediation (the branch condition `source().is_none()` is always true for Forge origins).
- `pull_requests.rs:745,777`: merge/close of a deleted Forgejo PR returns 502 `github_not_found` / "deleted on GitHub".
- `from_forgejo_url` (`fabro-types/src/pull_request.rs:232-247`) strips the instance subpath by segment *count* without verifying the prefix, so a URL under a different same-host subpath (`/other/...` vs configured `/forge`) parses — coordinates stay correct, display-level only.

## Verdict

The integration architecture is sound and I found no credential-leak or wrong-lane-dispatch defect. Finding **#1** is the one real functional bug: it breaks the single most common self-hosted Forgejo deployment (custom port, e.g. Gitea's default 3000) at token injection, clone classification, and PR creation, and the existing tests structurally cannot catch it. Finding **#2** is either dead code or a missed capability worth resolving deliberately. The rest are low-severity consistency/hygiene items.