I attacked the revised plan, focusing on whether my three prior defects were genuinely fixed and whether the new design decisions (`validate_with_endpoints`, `ForgeEndpoints`, cache discriminant, worker-side PAT resolution) introduce new blocking problems.

**Prior defects — verified fixed against the code:**
1. **Worker credential layer**: now correctly anchored at `runner.rs` `maybe_build_github_credentials` (1102–1132) with a forgejo twin feeding a new `StartServices.forgejo` field, and `spawn_env.rs` explicitly left unchanged with the reason stated (PEM-env is App-specific).
2. **Automation path**: `automation_materializer.rs` is now item 15 — endpoint-aware validation at line 241, forge-aware resolver, *plus* the `GitRepoCache.bare_dir` owner/repo collision fix I had not even raised, resolved without breaking existing github cache paths.
3. **The seam**: `ValidatedGitRunTarget.git` stays non-optional; bare `validate()` is grammar-only for forge targets and both endpoint-capable call sites (`handler/runs.rs:629`, `automation_materializer.rs:241`) switch to `validate_with_endpoints`. This type-checks and matches where settings actually exist. The sentinel-descriptor muddle is gone — declared-forgejo manifests keep the real remote URL and carry the marker.

**New probes I ran against this version — none blocking:**
- *Bare `validate()` as final word on a forge target*: the only validate call sites are the two now made endpoint-aware, the internal `RunTarget` wrapper, and github-only manifest construction; resume/fork paths replay persisted state rather than re-validating targets. Worst case is an implementer adding one more `validate_with_endpoints` call — ordinary judgment.
- *Worker origin availability for forge detection*: `maybe_build_github_credentials` decides from run settings alone; the forgejo trigger needs the run's origin, which the worker has via run state (publish.rs consumes the same data). Threading detail, not a design flaw.
- *`run.scm.provider` closed enum*: it's `Option<String>` compared with `eq_ignore_ascii_case` — no schema rejects `"forgejo"`.
- *Old-data compat*: additive serde fields with custom round-trip tests; old binaries reading new settings files is the repo's normal forward-compat posture.
- *git_bridge/services consistency*: the host-scoped `FORGEJO_TOKEN` helper and the `resolve_workflow_env` injection line up (the helper's env var is injected by item 24).
- Minor non-blocking notes: workflow PR tests may end up on `httpmock` like the existing github ones rather than the twin (the plan lists the twin as a dev-dep — either works), and the "mirroring the github ones in runner.rs's test module" phrasing assumes such tests exist; if not, new ones get written. Neither would cause rework.

Verification now covers the previously-untested spine: admission rejection, cache discrimination, docker clone-URL assertion, twin-backed PR pipeline, and the worker credential builder. Scope additions (`ForgeEndpoints`, cache discriminant, variant rename) are each correctness-motivated and flagged.

The plan names the right files against the code as it is, the central data flow compiles as specified, and remaining unknowns are within implementer judgment. It is sound enough to build from.

{"preferred_next_label": "approve"}