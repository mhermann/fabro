Review complete. I examined the full diff (109 files), with whole-file reads of the new `fabro-forgejo` crate, the sandbox credential machinery, and every handler that touches `FORGEJO_TOKEN` or derives URLs from user-controllable origins. Disk on this checkout is full, so none of this could be compiler-verified; everything below is from reading.

## Findings

**1. Cross-forge workflow-source resolution in the automation materializer (moderate)**

`lib/apps/fabro-server/src/automation_materializer.rs` — `materialize()` derives `forgejo_origin` from the **target's** provider:

```rust
let forgejo_origin = self.forgejo_instance_url.as_deref()
    .filter(|_| input.target.provider == ScmProvider::Forgejo)...
```

but in the separate-workflow-source branch it passes that target-derived value to `resolve_remote(...)` **without re-checking the source's provider**:

```rust
let remote = if repo == &target_repo { target_remote }
else { self.resolve_remote(CheckoutRole::WorkflowSource, repo, forgejo_origin.as_deref()).await? };
```

`ServerGitHubRemoteResolver::resolve` takes `Some(forgejo_origin)` as an unconditional signal to use the Forgejo path. So an automation with a **Forgejo target and a GitHub workflow source** (different repo) validates fine — `validate_with_scm` ignores the instance URL for GitHub providers — and then `forgejo_clone_url` clones `{instance}/{gh_owner}/{gh_repo}.git` **from the Forgejo instance using the instance PAT**, instead of from github.com.

What an attacker gets: on a forgejo-enabled deployment, a user who can create/modify automations and who controls a colliding owner namespace on the instance (e.g. registers `trusted-org` on the Forgejo instance) creates an automation whose workflow source names `trusted-org/trusted-workflows` on GitHub with a pinned SHA. Fabro materializes the *attacker's* same-named repo from the instance and executes its content as the workflow source, with the run's full credential set. The `cache_namespace_for(source_provider)` call also computes `None` for the GitHub-tagged source, so this clone lands in the **un-namespaced** bare cache under `cache_root/{owner}/{repo}.git` — exactly the collision the namespace feature was added to prevent — poisoning the shared cache so later github.com checkouts of the same slug can resolve attacker-chosen SHAs from the contaminated object store.

The reverse mismatch (GitHub target + Forgejo source) is correctly rejected at validation, so only this direction is reachable. Fix direction: dispatch `resolve_remote` on the *source's* provider (pass `source_origin` derived from `source_provider`, as the validation call right above already does), or assert provider consistency in `resolve`.

**2. `fabro-sandbox` test module does not compile (non-security, but it voids the verification claim)**

`lib/components/fabro-sandbox/src/push_credentials.rs:78-82` — `build_token_source` now takes three parameters (`github_app`, `forgejo`, `clone_origin_url`), but the existing test at line 677 still calls it with two: `build_token_source(None, Some(ORIGIN))`, and the subsequent calls pass `Option<&str>` where `Option<&ForgejoContext>` is expected. Rust has no overloading or default args; `cargo nextest run -p fabro-sandbox` will fail at compile. This explains why "full `cargo build --workspace --tests` passed" doesn't hold for the final tree — I could not re-verify by building (disk full), but the signature/arity mismatch is unambiguous.

**3. Low: unencoded owner/repo path segments in Forgejo API URLs (contained)**

`lib/components/fabro-forgejo/src/lib.rs` (`api_url`) interpolates owner/repo from `parse_owner_repo` into `/repos/{owner}/{repo}/…` without percent-encoding. The SSH spelling path in `url.rs::repository_path_of` returns the raw post-`:` substring, so an origin like `git@git.example.com:../x/repo` yields owner `..` and dot-segment normalization redirects the API path. Impact is capped: the base URL is always the operator-configured instance and the PAT is scoped to it, so this can only produce malformed or unintended requests *to the instance the credentials already belong to* — not cross-host exfiltration. Every consumer that turns these into local paths or persisted slugs does validate (`validate_path_component`, `GitHubRepositorySlug::try_new`), so no traversal there.

**4. Low/informational: TOML injection in `repo init` scm block**

`lib/apps/fabro-cli/src/commands/repo/init.rs::forgejo_scm_block` writes `owner = "{owner}"` from the raw SSH-URL substring; a remote containing `"` (legal in git config values) produces a corrupt `.fabro/project.toml`. Local file written at the developer's own request; fails validation later. Worth escaping, not a real attack surface.

## What checked out clean

- **Secret handling**: `FORGEJO_TOKEN` flows only through `ForgejoContext` (redacted `Debug`), `SecretString`, git2 in-memory `Cred`, or the env-read-at-invocation credential helper — never into git config files, argv, or logs; clone/set-url commands are `shell_quote`d and their errors pass through `redact_auth_url`; `GitAuthConfig::from_parts` registers token/base64/header for output redaction; diagnostics probe puts the token in a header and reports only status codes. No credentials committed in the diff (tests use mocks/env-gated live vars; `live_access.rs` requires env and is `live`-gated).
- **`origin_matches_instance`** is a sound trust boundary: parsed hosts compared case-insensitively, port-aware for HTTP(S), no suffix/substring bypass, `git@` scp-parse is narrow; embedded-credential and `\@`-trick URLs resolve to the real host.
- **Server authorization**: the new `GET /repos/forgejo/{owner}/{name}` requires `RequiredUser` and slug-validates inputs (alnum plus `-_.*`, `.`/`..` rejected); PR detail/merge/close keep `RequireRunScoped` and refuse stored links whose provider/origin doesn't match the configured instance, so no path exists that sends the PAT anywhere but the configured instance; run admission rejects unconfigured forgejo targets.
- **State/SQL**: the automation migration is additive with a CHECK-constrained column; all new SQL binds are parameterized; legacy rows default to GitHub; `SandboxSpec` keeps forgejo credentials runtime-only with scrubbed origins in records.
- **Web changes** are inert (controlled inputs, no injection sinks); deserializer forbids origin on GitHub links and requires it on forgejo links; `PullRequestLink::from_url` rebuilds credential-free origins.

Finding 1 is the one I'd block on; 2 should be fixed before merge since it breaks the crate's test build; 3 and 4 are hardening notes.