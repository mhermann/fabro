I've now read the full new crate, the webhook handler, the clone/preflight dispatch, the pipeline PR host, the wire-format changes, the install/diagnostics/CLI surfaces, and the manifest lane. Here is my correctness review.

## Findings

### 1. [Blocking] Explicit PR creation for Forgejo runs always fails — and the supervisor's Forgejo dispatch is unreachable dead code

- **Where:** `lib/apps/fabro-server/src/server/handler/pull_requests.rs:432-433` (`RunPrInputs::extract`) vs `lib/apps/fabro-server/src/server/pull_request_supervisor.rs:173-199`.
- **What's wrong:** `extract` GitHub-normalizes the origin and then requires a literal `github.com` host:
  ```rust
  let normalized_origin = fabro_github::normalize_repo_origin_url(origin_url);
  parse_github_owner_repo_from_url(&normalized_origin, "repo origin URL")?;
  ```
  `parse_github_owner_repo_from_url` (pull_requests.rs:46-66) returns a 400 `unsupported_host` for any non-github.com host. So for a run whose `git.origin_url` is on the configured Forgejo instance, extraction fails before the forge-aware host selection added in `attempt_pull_request_creation` (supervisor.rs:187-199, including its "a Forgejo run creates its pull request inline… " dispatch) can ever select `PullRequestHost::Forgejo`. The failure is recorded durably as a creation failure (supervisor.rs:175 → `append_pull_request_creation_failure`).
- **Trigger:** a successful Forgejo-origin run, then `POST /api/v1/runs/{id}/pull_request` (the web UI's create-PR button). Creation ends `Failed` with "Pull request operations support github.com only (got git.example.com)." The API's link/get/merge/close endpoints were all made forge-aware; this one path was missed, and the new forge branch below it is dead code.
- **Second half of the same defect:** even if `extract` were bypassed, line 191 runs `is_forgejo_origin` on `inputs.normalized_origin` — the **GitHub**-normalized URL, which mangles instance ports into the path (`https://git.example.com:3000/...` → `https://git.example.com/3000/...`). That is exactly the defect class the deep review fixed at the other three pre-normalization sites; this site was left GitHub-normalizing.

### 2. [Blocking-adjacent, scope ambiguity flagged] The local CLI run lane still rejects Forgejo checkouts; `fabro-manifest` was never touched

- **Where:** `lib/components/fabro-manifest/src/lib.rs:417-432` (`github_run_target` returns `None` for non-github.com origins) and `lib/apps/fabro-cli/src/commands/run/create.rs:165-167` (`run_target_for_environment` then hard-errors).
- **What's wrong:** The plan's governing answer says Forgejo runs enter "through the legacy manifest lane (full `git.origin_url`) and local working copies." The devils stage flagged exactly this crate (sites 382/387/418/449/517-520/1967) as finding #1 with "fix is the same dispatch pattern as every other site." The diff contains **zero** `fabro-manifest` changes.
- **Trigger:** `cd` into a Forgejo working copy and `fabro run <name>` with a clone-based environment → fails with "the caller Git checkout cannot be represented as a canonical GitHub run target" before any manifest exists. Also, `inspect_local_git` (fabro-manifest/src/lib.rs:382-388) still GitHub-normalizes the origin, so any manifest that does carry a port-bearing instance URL is corrupted upstream of the dispatch the implement stage just fixed downstream.
- Flagging honestly: scope B's enumerated list didn't name the local lane, so this may be a conscious deferral — but it contradicts the plan's declared entry lane, and the shipped code (clone source, preflight, publish) is otherwise ready to serve it.

### 3. [Low] `from_forgejo_url` strips the instance subpath without verifying the URL carries it

- **Where:** `lib/foundation/fabro-types/src/pull_request.rs:233-247`.
- **What's wrong:** The parser skips `instance_path.segment_count()` URL segments unconditionally after a host+port match. For instance `https://git.example.com/forge`, the URL `https://git.example.com/other/acme/widgets/pulls/9` parses as owner=`acme`, repo=`widgets`, and the stored link's recomputed `html_url` becomes `https://git.example.com/forge/acme/widgets/pulls/9` — a different URL than the one supplied.
- **Trigger:** the server link endpoint is safe (gated by `is_forgejo_origin`, which prefix-checks the full subpath), but the CLI `forgejo_link_for_url` (`lib/apps/fabro-cli/src/commands/pr/link.rs:40-49`) calls `from_forgejo_url` directly, so `fabro pr link <run> https://git.example.com/other/acme/widgets/pulls/9` against a subpath instance silently stores a link to a path the user never gave.

### 4. [Low] Forgejo `find_open_pull_request` drops the base branch

- **Where:** `lib/components/fabro-workflow/src/pipeline/pull_request.rs` (Forgejo arm, ~line 545) and `lib/components/fabro-forgejo/src/lib.rs:642-706`. The host method receives `base_branch`; the GitHub impl filters on it, the Forgejo impl matches only head ref + head SHA. On forges where one head branch has open PRs to two different bases, the reconcile/adopt path can adopt a PR targeting the wrong base. Narrow (requires exact head-SHA match, recovery path only), but the parameter is silently unused.

## Verified correct (no findings)

- **Webhook HMAC** (`forgejo_webhooks.rs`): constant-time `verify_slice`, all three header spellings, hex-decode failure and empty digest rejected, auth slot gated identically to GitHub.
- **Port-aware origin dispatch** — the deep-review fix is correctly implemented: `normalize_https_host_path` preserves numeric ports, and `decide_clone` / `parse_origin_owner_repo` / `run_repository_access_check_with` dispatch on the raw origin; GitHub branches are byte-identical to before. Scp-style URLs legitimately cannot match port-bearing instances.
- **Clone credential selection** — Docker's `CloneOrigin::Forge` and Daytona's `is_forge_origin` arms pick the static PAT correctly; `build_token_source` returns `None` for non-github.com hosts, so no spurious GitHub token mint or refresh happens for Forge origins.
- **Wire compatibility** — `PullRequestLink.forge` is omitted for GitHub (byte-identical), the deserializer cross-validates `html_url` using the *stored* forge string (not current config), so event replay stays stable even if the operator later changes the instance URL.
- **Token hygiene** — `SecretString`/`ForgejoCredentials` redaction, token confined to vault + point-of-use injection, bridge env values carry no secret, HTTPS-only instance validation (with `test-support`-gated HTTP for the twin).
- **Publish host selection** covers all three cases (forge configured+matching, forge configured+non-matching, forge absent) with sensible failure modes; `preflight_sandbox_spec`'s remaining GitHub normalization is inert because preflight forces `skip_clone = true`.
- The twin's `merge_when_checks_succeed`/`Do` shapes are internally consistent with the client — the residual live-API assumption (e.g. `merge_when_ready` on newer forges) is the already-recorded non-blocking note.

The two blocking findings share a root cause: three of the four call sites that GitHub-normalize an origin before forge dispatch were fixed in the implement round, but `RunPrInputs::extract` (finding 1) and the `fabro-manifest` lane (finding 2) were not, and finding 1 additionally strands otherwise-working forge-aware code behind a hard GitHub-only gate.