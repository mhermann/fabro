The threading hop is confirmed: plain struct fields (`github_app: Option<GitHubCredentials>` in `StartOptions`/`StartServices`), `.clone()` at each of five hops, consumed directly by `publish.rs`. That grounds the decision.

## Reasoning

**1. The central refactoring question was already answered by the human.** The vetted architecture question offered exactly this choice: additive parallel crate vs. shared `Forge` trait refactor across ~100 call sites. The answer was additive, with "GitHub code paths untouched" as the stated benefit. A refactoring stage that extracted a forge abstraction would override an authoritative answer — the main candidate is disqualified before any code reading.

**2. The seams this change needs are part of the change, not pre-work.** `PullRequestHost`, `CloneDecision::Forge`, `ForgejoInstance`, the parallel credential fields — none can exist meaningfully before the `fabro-forgejo` crate and its config types do. They are the plan's own Phase 0–2 content. Doing them "first" as a refactor would just be doing the implementation in the wrong order.

**3. The parallel-field threading is mechanical, not the plan working around the code.** Just verified in `operations/start.rs` and `publish.rs`: each hop is one struct field plus a `.clone()`. The actual dispatch logic is confined to two consumption points (`publish.rs` host selection, `initialize.rs` token injection). Five additive fields is the accepted price of zero GitHub regression risk, and the diff stays proportionate to the idea.

**4. Duplication is bounded and was explicitly accepted.** The `HttpClient`/`HttpMethod`/`HttpResponse` trio and `MockHttpClient` will be duplicated into `fabro-forgejo` (~150 lines). The human saw "some duplicated shape to maintain" in the option they picked. Deduplicating it into `fabro-http` would touch `fabro-github`'s public surface — against the answer — and only becomes clearly worthwhile when a third forge arrives. On record as a future item, not this one.

**5. No test is blocked by the current shape.** Mock-client unit tests, a twin-forgejo harness (the twin-github pattern exists to copy), spec/router conformance tests, and CLI scenario tests all reach the new behaviour with existing tooling.

**6. Names stay honest.** The plan already renames `GitHubRepoLayout` → `CloneRepoLayout`; `GitHubRepositorySlug` remains accurate for its github.com-only remaining uses; the automation lane legitimately stays GitHub-only per the scoping answer.

Refactorings considered and rejected as unrelated-to-this-change (on record): extracting the HTTP trio to `fabro-http`; splitting `pipeline/pull_request.rs` (2,046 lines) and `server.rs` into smaller modules; the forge-host trait (human-rejected — if ever revived it needs its own goal and conversation).

No refactoring stage has run in this workflow; this is the first and only decision point, and the answer is no.

{"preferred_next_label": "none"}