I verified the plan's riskiest claims against the code. Most hold up — worker credential propagation in particular does work the plan implies (tokens reach workers via the shared vault under `FABRO_HOME`, not via `spawn_env.rs`'s env allowlist, and settings reach them via `FABRO_CONFIG` at `worker_runtime.rs:101`; the plan's omission of `spawn_env.rs` is correct). But two findings are blocking.

## Blocking defect 1: the run-event/projection path silently drops the forgejo host — Phase 5's dispatch can never fire

The plan's server PR lifecycle (merge/close/get) dispatches on "the stored `PullRequestLink.host` matches the configured instance". But pipeline-created PRs never reach storage with a host:

- `fabro-workflow/src/pipeline/publish.rs` emits `Event::pull_request_created(&created.link, …)`.
- The converter (`fabro-workflow/src/event/convert.rs:1390-1410`) flattens that into `EventBody::PullRequestCreated(PullRequestCreatedProps)`, and **`PullRequestCreatedProps` (`fabro-types/src/run_event/misc.rs:365-376`) carries only `pr_url / pr_number / owner / repo` — no host**.
- The run projection (`fabro-store/src/run_state.rs:386-392`) reconstructs `PullRequestLink { owner, repo, number }` from those props — host is lost.

Consequence if built as planned: a forgejo PR created by the run pipeline is stored as a **github.com** link. `state.pull_request` is what `handler/pull_requests.rs` and the supervisor use for merge/close — they would issue `api.github.com` calls (with the GitHub PAT or none) against a forgejo PR number: silently wrong behavior on the exact lifecycle operations the scope promised. Worse, the broken links are persisted into append-only run-event logs, so this is expensive to reverse after the fact.

Required additions to the plan: add an optional, `#[serde(default, skip_serializing_if)]` `host` to `Event::PullRequestCreated` and `PullRequestCreatedProps`; derive it from the link in `Event::pull_request_created(...)` and `convert.rs`; set it in the `run_state.rs` projection. Also note `docs/internal/events-strategy.md` must be read before touching the `Event` variant (repo rule for this exact change).

## Blocking defect 2: the plan specifies the wrong Forgejo web-URL grammar

The plan dictates `html_url()` format `https://{host}/{owner}/{repo}/pull/{n}` for host-carrying links. `/pull/{n}` is **GitHub's** route; Forgejo/Gitea web routes are `/pulls/{n}` (which the plan itself relies on when it says `pr link` must accept `/pulls/`). Every PR URL Fabro renders for forgejo runs — in the web UI, `outcome.pr_url`, `fabro pr view` — would 404 on a real instance, and would disagree with the `html_url` the Forgejo API itself returns. Fix at plan level: host-carrying links use `/pulls/{n}`, and the forgejo PR path should prefer the API-returned `html_url` when constructing the link rather than synthesizing one.

## Minor notes (not blocking; implementer judgment calls)

- The `PullRequestLink` `Deserialize` html_url cross-check can't take an "allowed hosts" config parameter (the deserializer is config-free); it must validate against the record's own `host` field. The plan's `from_url_for_hosts` mechanism as written applies to the config-aware `pr link` handler, not to serde.
- `find_open_pull_request` via list+filter doesn't paginate; fine for small repos, worth a comment.
- The `SandboxSpec::Docker` construction site in fabro-workflow (where `forgejo` must be populated) isn't named; it's discoverable.

Both blocking defects are concrete, verified against the code, and cheap to fix in the plan now versus after event records exist in the wild.

{"preferred_next_label": "revise"}