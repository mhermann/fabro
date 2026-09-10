I examined the code shapes the plan will write into. Findings against the six signals:

**1. Working around the code?** No. The origin-classification ("github vs configured instance vs other") recurs across call sites, but each is a single `is_instance_origin` check selecting between two independent clients — that thin seam was the deliberate architecture decision, reaffirmed in the approved plan. There is no flag-through-five-call-sites problem: `observe_git_run_target` has exactly one production caller (`run/create.rs:161`), and `decide_clone`'s instance parameter threads through only the two provider constructors.

**2. Missing seam?** The candidate — a `GitForge` trait unifying `fabro-github`/`fabro-forgejo` behind one interface — was weighed and rejected twice during planning (it would refactor a stable, well-tested crate's entire public surface speculatively). The shared-seams approach (fabro-types + small `CredentialSource` enum) holds the change without it. That refactor would be its own piece of work with its own conversation if ever wanted.

**3. Duplication becoming triplication?** Verified: the `HttpClient`/`HttpResponse` transport abstraction exists exactly **once** today (`fabro-github/src/lib.rs:361,384` — `fabro-tracker` and `fabro-slack` define none). `fabro-forgejo` would be the second copy, ~50 lines of thin transport boilerplate. Extracting it into `fabro-http` now would churn `fabro-github`'s public API and mock infrastructure — violating the plan's own "GitHub paths byte-identical" safety property — to save marginal duplication. Below the bar.

**4. Unit outgrown?** Checked the riskiest: `open_pull_request` (`pipeline/pull_request.rs:592`) is 75 cohesive lines where the provider-varying calls (parse, verify, reconcile, create, auto-merge) are cleanly separable — a forge path parallelizes ~30 of them and the implementer can split into two functions sharing `build_pr_content` if it reads badly. `docker.rs` (3101 lines) takes one dispatch arm inside a contained clone function; `clone_source.rs` stays a coherent decision function; `server/handler/pull_requests.rs` (607 lines) holds parallel credential helpers fine. None is made unreadable by the planned addition.

**5. Tests can't reach the behavior?** No — the twin pattern (`test/twin/github`) is established and the plan adds `twin/forgejo` plus admission-level tests at manifest/CLI/workflow layers.

**6. Names that will lie?** Minor ones (`clone_github_repo` gaining a forge arm; `github_run_target` becoming origin-classifying) — the plan already renames the latter's siblings (`repo_layout`, `build_credential_source`); the rest are one-word renames naturally part of the change, not a pre-refactor.

**Conclusion:** every candidate either fails the bar or is already handled inside the plan's own steps. The only item worth putting on record for the future: extracting the shared HTTP transport trait into `fabro-http` becomes worthwhile if a third forge client ever appears — it is not needed for this change. When torn, say no; I am not even torn.

No refactoring is needed.

{"preferred_next_label": "none"}