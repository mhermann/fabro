I've read the full diff (`main...HEAD`, 96 files, ~4.6k insertions) and the key files in context: the new `fabro-forgejo` crate, sandbox clone/push/spec plumbing, workflow publish/pull-request/start, server handlers/preflight, config layers, fabro-types, CLI install/init, OpenAPI, and the web UI row. The overall shape is proportionate to the decided architecture (mirror crate, threading parallel to the GitHub path, spec/conformance kept green), and most of the diff is mechanical. The findings below are the pieces that will cost a future reader real time, ordered by cost.

## 1. Dead capability: `RepositoryProvider::Forgejo` is wired to nothing
`lib/foundation/fabro-types/src/repository.rs` adds `RepositoryProvider::Forgejo` plus `RepositoryRef::from_origin_and_source_with_forge`, but that constructor has **zero production callers**. Both places that build a `RepositoryRef` for run summaries — `lib/components/fabro-store/src/run_state.rs:1453` and `lib/apps/fabro-server/src/demo/mod.rs:1236` — still call the legacy `from_origin_and_source`, which passes `forgejo_base_url: None`. So a Forgejo run reports `provider: "git"` forever, while the OpenAPI enum (`fabro-api.yaml`) and the regenerated TS client now advertise `"forgejo"` as a value the server can never emit. Nothing in the web UI reads it either.

Simpler: pass the configured instance URL into the two summary builders, or drop the classification (and the enum value + client churn) until a consumer exists. As written it's a public API promise with no implementation path.

## 2. Three structs for one `(base_url, token)` pair, converted at every boundary
The same credential bundle exists as:
- `fabro_forgejo::ForgejoCredentials::Pat(String)` (token only),
- `fabro_workflow::operations::ForgejoRunCreds { base_url, token: String }` (`pipeline/types.rs`),
- `fabro_sandbox::ForgejoSandboxCreds { base_url, token: SecretString }` (`sandbox_spec.rs`).

Conversions appear in `operations/start.rs` (two identical `ForgejoSandboxCreds::new(...)` closures for the Docker and Daytona arms), `run_manifest.rs:489`, `server.rs:4175`, `publish.rs:174`, and `run_manifest.rs:939`. The GitHub precedent keeps **one** type (`GitHubCredentials`) shared across workflow, sandbox, and server — and both `fabro-workflow` and `fabro-sandbox` already depend on `fabro-forgejo`. One shared struct in `fabro-forgejo` (e.g. `ForgejoCredentials { base_url, token }`) would delete two types and five conversion sites, and the `String` vs `SecretString` distinction can live inside it as it does in `fabro-github/token_source.rs`.

## 3. `CloneDecision::GitHub`/`Forgejo` are structurally identical and consumed identically
Both variants hold the same four fields; both providers match them in a single combined arm (`docker.rs:1883`, `daytona/mod.rs:1591`). The only differentiation is `CloneDecision::is_forgejo()`, which has exactly **one** production caller — the Daytona gate at `daytona/mod.rs:1597` that re-derives the base URL, which `repo_layout_for_origin` then re-validates via `is_forgejo_origin` anyway (Docker skips the gate and passes the URL unconditionally, proving it's redundant). Simpler: one `Cloned { provider, origin_url, branch, tag, commit_sha }` variant, which deletes `is_forgejo()` and the Daytona double-classification while preserving `decide_clone`'s validation.

## 4. "Is this URL on the instance?" is implemented three times with divergent semantics
- `fabro_types::is_forgejo_origin` — parsed, host compared case-insensitively, base-path aware.
- `fabro_forgejo::parse_forgejo_owner_repo` — raw string `strip_prefix` of the normalized base, case-sensitive.
- `forgejo_pull_request_link_from_url` (`fabro-types/src/pull_request.rs`) — `host_str` equality plus segment skipping.

The classifier and the parser can disagree: an origin like `git@Forgejo.Example.com:acme/w.git` passes `is_forgejo_origin` (URL parsing lowercases the host), so `decide_clone`/publish/supervisor select the Forgejo path, then `parse_forgejo_owner_repo` fails because its pure-string prefix compare never lowercases — producing "Not a URL on the configured Forgejo instance" for an origin the classifier just claimed. At minimum the parser should reuse the same host/path comparison; ideally one matcher serves all three sites.

Related, within the accepted URL-helper duplication: the two `normalize_repo_origin_url` copies have already silently diverged — GitHub's includes a `normalize_https_host_path` step (sanitized `https://host:path` shapes) that Forgejo's copy lacks, and call sites already mix them (`repo/init.rs:209-212` runs `fabro_github::ssh_url_to_https` then `fabro_forgejo::normalize_repo_origin_url`). If duplication stays, the copies should behave identically or document which differences are intentional.

## 5. `run_forgejo_origin_check`: unreachable arm and a no-op (`run_manifest.rs:912, 928`)
`Ok(None) => false` is unreachable — the function returns early when `forgejo_base_url()` is `None`, and `forgejo_credentials()` returns `Ok(None)` only in exactly that case. Worse, if it ever did run it returns a preflight failure **without pushing any `CheckResult` row**, i.e. a silent failure in the report. And `let _ = creds;` is a no-op to silence an unused binding. Simpler: match `Ok(Some(_))` and delete the no-op.

## 6. `maybe_build_forgejo_credentials` is a pure pass-through (`runner.rs:1111`)
Single caller at line 147, body is `build_forgejo_credentials(vault)`. The GitHub sibling of the same name contains real policy logic, so the wrapper buys only naming symmetry. Call `build_forgejo_credentials(&vault_guard)?` directly.

## 7. Minor dead API in `fabro-forgejo`
- `ForgejoContext::creds()` (`lib.rs:118`) has no callers anywhere.
- `RepositoryInfo.html_url` is never read in production; `full_name` is read only by its own test. The repo-access probe consumes only `default_branch` and `private`.

Trim these or give them consumers.

## 8. Error routing by rendered string in `get_forgejo_repo` (`system.rs`)
The handler branches on `rendered.contains("not found")` and `contains("token rejected")` against `get_repo`'s bail messages — rewording the client message silently misroutes the response. The crate already established the typed-error pattern (`PullRequestApiError`); a small typed result for `get_repo` (or matching on status inside the client) removes the string coupling.

---

**What's appropriately simple, for the record:** the env-var/settings/config-layer additions, the OpenAPI edits with replacement types and parity tests, the `git_bridge` Forgejo mirror, `push_credentials` static-PAT reuse, the CLI install command (it correctly reuses the pending-write/rollback machinery rather than duplicating it), the `PullRequestRemote` enum replacing `OpenPullRequestRequest.github`, the demo-router stub, and the web UI row gated on server support. None of those need restructuring. Items 1–4 are where I'd push back before merge; 5–8 are small cleanups.