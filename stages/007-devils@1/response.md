I attacked the plan against the repository as it actually is. Findings, then the verdict.

## Defects found

**1. `fabro-manifest` is omitted entirely.** `lib/components/fabro-manifest/src/lib.rs` consumes `normalize_repo_origin_url` and `parse_github_owner_repo` at sites ~382, 418 (`parse...ok()?` — silently drops owner/repo for non-GitHub origins), 449, 517–520, and 1967 when building run manifests from a local checkout. The plan declares the manifest lane (full `git.origin_url`) the *primary entry* for Forgejo runs, yet never audits the crate that constructs those manifests. A local `fabro run` in a Forgejo checkout may produce a manifest with the origin but missing the owner/repo identity downstream PR creation needs. Fix is the same dispatch pattern as every other site — additive, test-catchable, no decision reversal.

**2. Two threading links are unnamed.** `run_manifest.rs:602` (`parse_github_owner_repo` in the git-coordinate path, before preflight) and `fabro-workflow/src/operations/start.rs:114` (`RunServices.github_app`, which feeds `publish.rs`). The plan names `run_options.rs`, `services.rs`, `publish.rs`, and says "thread into worker run options," so the chain is implied — but these two specific links are where a Forgejo run would fail first. Greppable in minutes.

**3. The Gitea→`PullRequestGithubDetail` DTO mapping is never called out.** Forgejo's PR JSON is not GitHub-shaped (different `state` vocabulary, nested head/base objects). `get_pull_request` must convert into the shared `PullRequestGithubDetail`. The plan's wording ("handlers branch identically") implies it, but the conversion is the riskiest parsing code in the crate and deserves a named test fixture.

**4. Twin-forgejo inherits my API assumptions.** Both the mock tests and the twin encode the same possibly-wrong beliefs about the Gitea-lineage REST surface (merge endpoint parameters, auto-merge shape, token-in-URL username). Mock tests would pass even if the real API differs. Mitigations exist in the plan (explicit auto-merge fallback, env-gated live tests, mandatory manual E2E against a real Forgejo docker) — but the live tests are opt-in, so the manual pass is the real gate and must not be skipped.

**5. Web run-creation path unverified.** PR chips are `html_url`-driven (fine), but nothing in Phase 5 confirms the web/playground run-creation form passes a Forgejo URL through the manifest lane rather than validating for github.com. If it validates, a small addition is needed; the plan says "verify" for chips but not for creation input.

**6. Rollback nuance:** adding `[server.integrations.forgejo]` to settings.toml means downgrading to an older binary will reject the section (settings structs are strict). Standard posture for this repo (new keys always do this), not a plan defect.

## Bar check

None of these forces redoing work or reverses a decision: the architecture (additive crate, single instance, PAT, dispatch points) is unaffected; every gap is an additive call-site audit or a named test within files/patterns the plan already establishes, and the scenario test in Phase 6 would surface each one. Per the stated bar — blocking means wrong work or expensive-to-reverse decisions — these are implementer judgment calls plus review-stage catches, not revision triggers. The `PullRequestLink.forge` wire addition is explicitly flagged, justified, and cheap to remove if unwelcome.

The plan is sound enough to build from, with the six notes above carried into implementation.

{"preferred_next_label": "approve"}