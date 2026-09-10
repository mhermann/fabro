I looked for the specific failure modes — workarounds, missing seams, triplication, overgrown units — by reading the code the plan will sit beside, not the plan.

**What I found:**

1. **The triplication risk is mostly already solved.** Docker and Daytona's file operations are *not* copies of each other: Docker rides its native tar/exec APIs (`download_file_bytes`, `upload_bytes_to_container`, `docker_exec`), Daytona rides its SDK's fs service (`fs_svc.download_file`, `create_folder`). There is no exec-based file-ops module existing twice that Kubernetes would copy a third time. The genuinely shared remote-sandbox logic — bash probe, walk-command building/parsing, path resolution, `setup_git_via_exec`, git retry, push credentials — was already extracted into `sandbox.rs`/`clone_source.rs`/`git_retry.rs` as `pub(crate)` helpers, cfg-gated `docker | daytona`. The repo has demonstrably maintained this seam; extending the gates with `kubernetes` is the designed extension point, not a workaround.

2. **The remaining duplication is small-scale.** A K8s exec-based `list_directory`/`file_exists` will resemble Docker's (~15 lines each, different transport). Extracting an exec-file-ops mixin would serve 2 of 3 providers and save ~100–150 lines — below the "substantially harder/larger/riskier" bar; the implementer can extract in-change if it turns out clean.

3. **The parallel `SandboxProviderKind`/`EnvironmentProvider` enums are real debt** — every provider addition touches both plus a `From` impl plus two `is_clone_based`s. But this change's cost for that debt is ~10 mechanical lines, and unifying the enums is a workspace-wide change with OpenAPI type-ownership implications — its own piece of work with its own conversation. Named for the record; not this change.

4. **`SandboxSpec` variants repeat clone fields** (config/github_app/run_id/origin/branch/tag/sha per variant); a shared `CloneContext` would help, but restructuring the two existing variants churns every construction site in three crates for no reduction in risk to this change. Not needed.

5. **No overgrown unit gets grown further** — the K8s provider is new files; `docker.rs`/`daytona/mod.rs` are untouched. The plan already embeds the two correctly-sized in-change refactors it needs (`ReconnectCredentials` param consolidation, BASH_ENV scrub helper move to `sandbox.rs`), which is the right size for them.

6. **Testability is fine** — new modules are pure builders/mappings; the agent protocol is testable at process level; no seam is missing that verification depends on.

The change slots into seams the repository already has. Refactoring first would delay the work without making it meaningfully smaller or safer.

{"preferred_next_label": "none"}