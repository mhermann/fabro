Everything is verified. The Daytona and Docker clone paths both consume the decision's `origin_url`, which now carries the port correctly through `embed_token_in_url`. Here's the complete summary.

## Summary

The deep_review stage found one blocking defect: **port-bearing Forgejo instances (including Forgejo's default port 3000) failed everywhere** because `normalize_https_host_path` swallowed the port into the path, and three call sites pre-normalized origins with the GitHub normalizer — which contains the identical mangle — before the forge check. I implemented the review's fix point by point, plus one adjacent same-class site found while tracing.

### Files changed

**`lib/components/fabro-forgejo/src/lib.rs`** — the core fix (review point 1): `normalize_https_host_path` is now port-aware. When the segment between `:` and the first `/` is all ASCII digits, it's treated as an instance port and the URL passes through untouched; only the sanitized scp-like `host:owner/repo` shape (non-numeric segment) is rewritten to `host/owner/repo`.

**`lib/components/fabro-sandbox/src/clone_source.rs`** — review point 2:
- `parse_origin_owner_repo` now runs `is_forgejo_origin` on the **raw** origin (it normalizes internally) and dispatches to `parse_forgejo_owner_repo` there; the GitHub branch is unchanged, still parsing the GitHub-normalized URL.
- `decide_clone` likewise dispatches on the raw origin, then stores each branch's own canonical form: `normalize_forgejo_origin_url` (port-preserving) for `CloneDecision::Forge`, the GitHub normalizer for `CloneDecision::GitHub` — so the clone URL both Docker and Daytona consume is correct.
- `clean_clone_origin_for_record` — **same defect class, not on the review's list**: it GitHub-normalized unconditionally for the persisted sandbox record, so a port-bearing Forgejo origin would be recorded (and later shown on reconnect/inspect) as `https://host/3000/owner/repo`. It now takes the configured instance and dispatches normalizers exactly like the other sites. Both `sandbox_spec.rs` call sites updated.

**`lib/apps/fabro-server/src/run_manifest.rs`** — review point 2: `run_repository_access_check_with` runs the forge check on the raw `git.origin_url`; the forge branch reports the forge-normalized origin (credential-stripped, port-preserving) in its details, and the GitHub branch below is byte-identical to before.

**Tests** (review point 3, plus one extra):
- `fabro-forgejo/src/tests.rs` — 4 new tests: port-bearing instance keeps port/host/api-base; normalizer preserves the port for the `https://`, `ssh://`, and credential-bearing forms; `is_forgejo_origin` matches its own port-bearing origins only (sc-style URLs legitimately never match); `parse_forgejo_owner_repo` on a port-bearing instance.
- `clone_source.rs` tests — 3 new: `decide_clone` → `CloneDecision::Forge` with port, `clone_repo_layout` forge parse with a credential-bearing port URL, and record normalization keeping the port.
- `run_manifest.rs` tests — 1 new: a port-bearing origin with a matching instance dispatches to the forge preflight branch (observable via its distinct remediation, using an instant-refusal loopback port), not the supported-origins error; the GitHub remote probe is never invoked.
- `test/twin/forgejo/tests/client_ops.rs` — 1 new end-to-end test: the twin's inherently port-bearing `http://127.0.0.1:<port>` instance URL flows through `is_forgejo_origin`/`parse_forgejo_owner_repo` ��� this closes the review's note that twin tests never passed an origin URL through the normalizer.

### Adaptations and deviations

- **`clean_clone_origin_for_record`** was fixed beyond the review's named sites — I believe this is in the spirit of the finding ("port-bearing instances do not work at all"): it was the persisted-record leg of the same root cause, and leaving it would surface corrupted origins in `RunSandboxRuntime` metadata.
- **`preflight_sandbox_spec`'s** GitHub pre-normalization (line ~1039) was left unchanged deliberately: preflight sets `skip_clone = true`, so `decide_clone` short-circuits to `EmptyWorkspace` before ever inspecting the origin — verified inert for ports, and the review's instruction was to keep the GitHub branch exactly as today.
- **One pre-existing, unrelated issue left alone:** `install::tests::write_artifact_store_metadata_*` and two `serve::tests` object-store tests fail only in full parallel `cargo test -p fabro-server` runs — confirmed failing identically on the base commit (they pass in isolation). Test-isolation flakiness, out of scope for this fix.

### Validation

- Full `cargo build --workspace` clean; `cargo +nightly-2026-04-14 fmt --check --all` clean; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings` clean.
- Tests: fabro-forgejo 35/35, fabro-sandbox (docker) 218/218, fabro-server 910 + the new dispatch test (the 3 flaky failures are the pre-existing ones above), fabro-workflow 1445/1445, twin-forgejo 18/18, and a full workspace run with zero failures.