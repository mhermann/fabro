The plan is mostly sound — the layering, the stub-first ordering, the exec/sentinel design, and the repo-specific wiring points (registry, preflight, capability validation, feature unification into the shipped binary) all check out against the code. But I found one genuine miss with a hidden coupling, one internal design contradiction, and two concrete traps the plan leaves to chance. The first two meet the bar; the rest I'd fold into the revision for free.

## Blocking defects

**1. The web environments surfaces are missing entirely — including a semantic coupling that breaks automations UI gating.**
The plan's web step lists only `settings-sandboxes.tsx`. But the primary way users create environments is the environments UI, and it hardcodes the provider set in files the plan never mentions:
- `apps/fabro-web/app/lib/environment-providers.ts` — `CREATABLE_PROVIDERS = [DOCKER, DAYTONA]`. `settings-environments.tsx` builds the "New environment" picker as `CREATABLE_PROVIDERS.filter(p => providers[p].enabled)`. Without adding `KUBERNETES`, a kubernetes environment **cannot be created from the web UI at all**, even with the provider enabled server-side. The REST API path works, which makes this worse, not better — it ships a provider reachable only by curl.
- The same file exports `isCloneBasedEnvironment(environment)` = membership in `CREATABLE_PROVIDERS`. That is a **hidden coupling**: the server treats `kubernetes` as clone-based (`is_clone_based() == true`, so Git targets and automations are legal), while the web would treat kubernetes environments as *not* clone-based — silently excluding them from Git-targeted automations in the UI. Same enum, two contradictory answers in two layers.
- `apps/fabro-web/app/components/environment-form.tsx` — the form has an `ImageSource = "image" | "dockerfile"` union with a Dockerfile editor. For kubernetes, dockerfile is rejected by the config layer (per the plan and the Q2 answer), so the form must take a kubernetes branch that hides/disables the Dockerfile option rather than letting users compose a request that 422s.
- `automation-form.tsx` appeared in the daytona-reference grep and likely consumes `isCloneBasedEnvironment` — needs the same one-line update.

This is exactly the "missing case the goal implies" + "hidden coupling the plan does not mention" class: the plan promised every file, and the gap isn't cosmetic — it's the creation path for the thing being built.

**2. The plan contradicts itself on the sandbox identity (`sandbox_info` vs `runtime.id`).**
The plan says `sandbox_info()` returns `kubernetes:<namespace>/<pod>`. But `SandboxSpec::to_run_sandbox_instance` persists `sandbox.sandbox_info()` as `RunSandboxInstance.runtime.id`, and the plan's `reconnect.rs` arm passes `runtime.id` straight through as the **pod name** (the registry's `get(id)` and `details.rs` face the same string). Docker returns the bare container id for exactly this reason. As written, the first reconnect of a kubernetes run feeds `kubernetes:ns/pod` into a Pod GET and fails. The plan must pick one canonical id — bare pod name (namespace re-derived from inference at reconnect, matching how no other credential is plumbed) or a namespaced name parsed at every consumer — and say so.

## Non-blocking corrections to fold in while revising (named so they're deliberate, not discovered)

- **Pod names must be RFC 1123 lowercase**: `run_id` is a ULID (uppercase base32); `fabro-<run_id>` as written will be rejected by the API server. Lowercase it (label *values* may keep the original ULID).
- **`clone_source` module cfg gate** in `src/lib.rs` (`any(feature = "docker", feature = "daytona")`) needs `kubernetes` too — the plan widens `sandbox.rs` and `managed_labels.rs` but misses this one (compiler-forced, but it's in the file list).
- **`spawn_stdio_process` must not use the `__FABRO_RC__` stdout sentinel** — MCP stdio is length-prefixed JSON and a marker line would corrupt the stream tail. Detect termination via attach-stream close instead. The plan implies this but doesn't state it; state it so the implementer doesn't unify the two paths.
- **kube feature flags**: the exec/attach WebSocket requires `kube`'s `ws` feature plus a TLS feature (`rustls-tls`, matching the workspace's rustls-only stance). Worth one line so the `cargo add` doesn't ship a provider that can't exec.

## Checked and not blocking

- **CI-verification gap on exec/streaming/tar** (only live-gated, never-in-CI tests cover the transport): matches the repo's existing Docker convention exactly (`docker_streaming.rs` is `#[ignore]`d and CI's `--run-ignored only` covers only fabro-agent/fabro-llm/twin-openai), and the plan discloses it plus a concrete manual kind smoke. By repo norms this is acceptable.
- **httpmock-vs-websocket uncertainty**: flagged in the plan with a workable fallback (mock tower service, which is kube's documented testing pattern).
- **OpenAPI required-field addition** (`kubernetes` in `ServerSandboxProvidersSettings`): precedent-identical to the daytona field; changelog covers it.
- **Scope additions** (`ssh_access_command` display string, diagnostics card, NetworkPolicy enforcement): all surfaced to the human in the decided-list; nothing quietly added.

Defects 1 and 2 would ship wrong behavior from a plan followed faithfully — one makes the provider unreachable/gated incorrectly in the primary UI, the other breaks resume on first contact. Both are cheap to fix in the plan now and expensive to discover late.

{"preferred_next_label": "revise"}