This is my second pass over this plan. Round one found two blocking defects (missing web environments surfaces with the automations gating coupling; the `sandbox_info`/`runtime.id` contradiction) and four named corrections. I've re-attacked rev 2 against the code rather than my memory of it, hunting specifically for new blocking defects and for round-one concerns that were addressed only superficially.

## Round-one points: verified as genuinely fixed

- **Web surfaces**: `CREATABLE_PROVIDERS` in `environment-providers.ts` is the single source both `settings-environments.tsx` (line 71 picker filter) and `automation-form.tsx` (lines 126/301) consume — I re-read both consumers; the one-file fix claim is correct, and `parseCreatableProvider`'s Docker-default (verified in the form source) is patched. The ordering is also right: client regen (Step 7) precedes the web step so `providers[provider].enabled` typechecks.
- **Identity**: bare pod name everywhere, namespace re-inferred — consistent across `to_run_sandbox_instance`, `reconnect`, `details`, and registry `get`. The reconnect round-trip is now in the live test list.
- **Pod-name lowercasing, `clone_source` gate, no-sentinel-for-stdio (`StdioProcessTermination::exited(None)` is legal — verified against the type), kube feature flags**: all folded in.

## New attack — what I checked and what remains

I went looking for fresh blocking defects in the risky spots:

- **Sentinel vs. binary/streamed output**: the hold-back-the-final-line design survives the nasty cases (no-trailing-newline output, binary tar payloads where the marker must be stripped before bytes reach the downloaded file, marker-collision-in-output). Fiddly, but unit-testable as planned. Implementation-level, not plan-level.
- **Dependency compatibility**: kube 2.x's hyper/tower stack vs. the workspace's reqwest 0.13/rustls tree — any conflict is a build-time `cargo` failure with a clear error, resolved by adjusting the kube version/features. Annoying, not redo-work.
- **Exhaustive-match coverage**: cross-checked every non-test `SandboxProviderKind`/`EnvironmentProvider` match site from my earlier repo-wide grep against the plan's file list — `automations.rs`, `handler/sandboxes.rs`, `run_files.rs`, `fabro-api/build.rs` use conversions/comparisons/Display only and need no arms; everything with a real match is listed. The compiler forces the rest.
- **Registry/reconnect namespace consistency** under `Config::infer()`: same-process workers and server restarts re-infer identically. Sound.
- **`restartPolicy: Never` + node loss**: pod goes Failed and stays; resume errors cleanly. Documented behavior, not a defect.
- **Install-wizard TS types**: `InstallSandboxInput` stays `docker|daytona` so no install-flow breakage; any web-side exhaustive switch on the widened enums fails `bun run typecheck`, which is in the verification list.

**Residual non-blocking items** (ordinary judgment calls for the implementer, named so they're not discovered late): the exact `k8s-openapi` version feature must match the kube build or compilation fails loudly; the environment-form's per-provider network-mode UI needs a look when implementing the kubernetes branch (server validation is authoritative either way); the marker parser must anchor on the *last* line only. None of these would produce wrong shipped behavior if handled at implementation time, and all are caught by the planned unit/typecheck gates.

The CI-coverage gap for exec/streaming/tar transport is the same concern I raised in round one; it matches this repo's Docker convention exactly (`docker_streaming.rs` is `#[ignore]`d; CI's `--run-ignored only` covers only fabro-agent/fabro-llm/twin-openai), the plan discloses it, and it specifies a concrete manual kind smoke. Re-raising it would be re-litigating an accepted, convention-consistent tradeoff.

No new blocking defects; the plan's file enumeration is now consistent with the repository as it actually is, its internal design contradictions are resolved, and the remaining unknowns fail loudly at build/test time rather than shipping silently wrong.

{"preferred_next_label": "approve"}