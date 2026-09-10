The answers covered the forks that mattered — deployment shape, config surface, and the clone/security contract. What's left (client library choice, internal pod layout) is implementation detail, not a material fork. Proceeding.

What I now understand the human wants:

- **A new Kubernetes provider in `fabro-sandbox`** implementing the existing `Sandbox` trait, as a peer of Docker and Daytona — not a replacement. Docker stays the default in `defaults.toml`; Kubernetes is opt-in via provider selection.
- **Same clone-based workspace contract as the other remote providers:** GitHub origin → clone into `/workspace`; non-GitHub origins fail unless `skip_clone = true`; absent origin or `skip_clone = true` → empty workspace. Exact-commit support follows the established rules: non-empty branch plus SHA, shallow-fetch of the SHA, checkout, admitted branch pointed at the commit, HEAD verified, and never a fallback to a newer branch HEAD. The Kubernetes transport stays distinct — shared code limited to the trait and common abstractions, no reuse of Docker/Daytona transport internals.
- **Config through provider settings:** kubeconfig path (or in-cluster detection), context, namespace, image, and resource limits — no new top-level config concepts.
- **Trusted-caller security posture**, same as the Docker provider: the operator is responsible for cluster access and isolation; we don't add multi-tenant hardening in this iteration.

Decisions I'm making on the human's behalf (small gaps, not worth another round):

- Use the Rust `kube` crate in-process rather than shelling out to `kubectl`, so the provider behaves like the other in-process transports and streams exec output natively.
- One Pod per sandbox with the clone/checkout done in an init container; the agent runs commands via the exec API.
- Track sandboxes with labels for ownership/cleanup, mirroring how Docker identifies run containers.
- Tests: unit tests with fixture manifests, plus env-gated live e2e tests behind the existing `e2e` profile rather than requiring a cluster by default.

{"preferred_next_label": "enough"}