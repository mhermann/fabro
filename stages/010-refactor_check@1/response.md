I mapped `docker.rs`'s actual structure (the 3,101-line module the plan builds beside) and re-examined the shared surface in `sandbox.rs`, looking specifically for the plan working around the code, missing seams, and duplication about to multiply.

## What I looked at and found

**The genuinely shared layer already exists and is load-bearing.** `sandbox.rs` already holds the provider-neutral logic as free functions over `&dyn Sandbox` — `setup_git_via_exec`, `git_push_via_exec`, `fetch_source_run_ref`, `build_remote_walk_command`/`parse_remote_walk_output`, `resolve_path`/`join_sandbox_path`, the bash-probe contract — all cfg-gated `any(docker, daytona)` and all consumed by both existing providers. The plan's kubernetes module reuses these directly. That is the seam, and it's the right one.

**The one real duplication candidate — and why it doesn't trigger a pre-emptive refactor.** Docker's clone-with-retry block (`clone_github_repo`, `retry_git_transfer`, `run_exact_local_git_command`, `git_clone_command`, the failure classifiers — roughly lines 787–1125 and 1588–1615, ~400 lines) is provider-neutral logic that a naive kubernetes implementation would copy. But the repo's own history shows exactly how this is meant to evolve: `git_push_via_exec` and `setup_git_via_exec` were extracted into `sandbox.rs` over `&dyn Sandbox` *when the second consumer arrived*, and the plan's Step 5 plus its existing edits to `sandbox.rs` accommodate doing the same for the clone block **during implementation, guided by the real second consumer**. Extracting it *now* — before kubernetes exists — means guessing the seam: does the kubernetes side need Docker's stop-file/pid-file exec-control mechanism (plan says no — kill-by-pgid), the `docker_exec_shell` variants, or just trait-level `exec_command`? A pre-emptive extraction risks carving the wrong boundary and getting reworked. This is the documented-in-AGENTS.md, contract-heavy clone code; extracting it behavior-preserving-first with only daemon-gated tests is its own risk. The implementation-time extraction keeps the kubernetes diff honest and is reviewable as "move, don't modify."

**Everything else the plan touches is the repo's established extension pattern, not a workaround:**
- The ~10 cfg-gate widenings (`any(docker, daytona)` → `+kubernetes`) are mechanical; Cargo has no feature aliases, so no cheaper seam exists.
- Enum arms across `sandbox_spec.rs`, `provider.rs`, `reconnect.rs`, `details.rs`, `terminal.rs`, `start.rs`, `run_manifest.rs` are exactly how Daytona was added — extensibility by variant, compiler-enumerated.
- File ops (read/write/grep) are thin per-provider shells over each transport and are *already* duplicated between Docker and Daytona; a third thin implementation is the architecture's price, with the nontrivial command builders already shared.
- Exec transports are inherently provider-specific (bollard exec API / Daytona session / kube WebSocket) — a unifying "transport trait" would be speculative abstraction, not a needed seam.
- Testability holds in the current shape: the plan's unit tests target pure functions (wrapper builder, sentinel parser, policy construction) inside the new module; transport-dependent behavior is live-gated per repo convention, same as Docker today.
- No unit the plan grows is already too long (docker.rs/daytona aren't grown; sandbox.rs gains only cfg widenings and possibly the clone extraction, which would *shrink* docker.rs), and no name stops matching (`clone_source`, `REMOTE_BASH`, `from_environment` all stay honest).

## Named but declined

- **A unified exec-transport abstraction across Docker/Daytona/kubernetes** — worth its own goal someday (Daytona's 5,093-line everything-itself module is pre-existing debt), but unrelated to making this change land, and too large to fold in.
- **The clone-block extraction as a standalone pre-refactor** — deliberately declined for the seam-guessing reason above; it belongs inside this change's Step 5, mirroring the `git_push_via_exec` precedent.

No refactoring is needed first: the implementation is not substantially harder without one, the duplication it would introduce is bounded and has an established in-repo resolution pattern, and the change is testable in the current shape.

{"preferred_next_label": "none"}