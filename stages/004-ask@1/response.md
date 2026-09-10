All five answers are back. Restating them for the record — this is what the planning stage will build from:

## Answers and what they mean

1. **Exec transport → In-pod agent sidecar.** Each sandbox pod runs the user's image plus a Fabro agent sidecar; the provider talks to the agent (Daytona-toolbox-style) for exec streaming and stdio processes. Consequences: Fabro must publish an agent container image as a new release artifact (the cluster must be able to pull it); the server/worker needs Kubernetes RBAC only for pod lifecycle (create/get/list/delete — **no** pod-exec RBAC); streaming/PTY complexity moves into the agent protocol.

2. **Image story → Prebuilt image refs only.** `image.docker` is honored as a pod image reference; `image.dockerfile` is rejected at config validation for Kubernetes environments with a clear remediation error (build and push elsewhere). No registry settings, no build machinery, no snapshot identity.

3. **Capability scope → Core parity.** Full `Sandbox` trait (exec streaming, files, walk/grep/glob, git setup, push credentials), provider inventory list/get/delete, reconnect, config/policy/UI/API wiring, docs. **No** PTY web terminal, **no** preview URLs/VNC, **no** install-wizard entry — those return explicit "not supported" errors, exactly as Local does for terminals today.

4. **Cluster targeting → Ambient only.** The kube client resolves the cluster from the in-cluster ServiceAccount or the process's default kubeconfig; zero new settings.toml schema; a single implicit cluster per server. Workers do **not** inherit ambient env (the fail-closed allowlist stays) — pod lifecycle credentials travel via the vault/FABRO_HOME pattern only where needed.

5. **Network modes → Reject at validation.** Kubernetes environments accept only `network.mode = "allow_all"`; `block` and `cidr_allow_list"` are rejected in `fabro-config` validation with the Docker-precedent error style. No NetworkPolicy objects are created.

Combined shape: a clone-based, pod-per-run provider with a published agent sidecar image, prebuilt-image-only environments, ambient single-cluster discovery, core capability surface, and allow-all-only networking — everything else (naming `kubernetes`, `/workspace`+`/repos` layout, bash+git image contract, resource mapping requests=limits + ephemeral-storage, stop=delete-pod with git-checkpoint-based resume, managed-label inventory safety, vault credential transport, `#[ignore]`/env-gated live-cluster tests, additive OpenAPI/UI updates, docs + changelog) was already settled in the previous stage and stands.