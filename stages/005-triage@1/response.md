The answers I have are enough to build from. The remaining open points (kube-rs vs. shelling out to `kubectl`, Pod-per-run vs. Job vs. namespace-per-run) are implementation choices I can settle myself from repo conventions rather than product ambiguity — two engineers given these answers would converge on materially the same provider.

What I understand the human wants:

- A **Kubernetes sandbox provider** in `fabro-sandbox`, joining `local`, `docker`, and `daytona` behind the same `Sandbox` trait, so runs can execute in containers scheduled by a Kubernetes cluster (primarily for server deployments) instead of the operator's Docker daemon.
- It should follow the established **clone-based provider contract**: GitHub-only origins cloned into `/workspace`, optional exact commit always paired with a non-empty branch (shallow-fetch the SHA, pin the branch at it, verify HEAD, never fall back to branch HEAD), empty workspace when the origin is absent or `skip_clone = true`, and present non-GitHub origins fail setup.

Decisions I'm making on their behalf:

- **Client**: use `kube` (kube-rs) with `Config::infer` — in-cluster service account when running inside a pod, local kubeconfig otherwise. Cluster access, RBAC, and namespace provisioning are operator responsibilities, mirroring the existing stance on Docker daemon access.
- **Shape**: one Pod per sandbox (namespace, image, resource requests/limits, and optional node/affinity knobs from provider config) — not a Job controller or namespace-per-run orchestration in this iteration. Keep the clone transport distinct from Docker's, per the repo guidance that provider transports stay separate.
- **Config**: new `kubernetes` provider selectable via `defaults.toml`; Docker remains the default so this is opt-in.
- **Tests**: unit tests with a fake kube client; live-cluster tests gated behind the `e2e` profile with credentials from `.env`, honoring the `test_support` feature boundaries so no test shims leak into production builds.

{"preferred_next_label": "enough"}