Review complete. I read the full new provider (`kubernetes.rs` ~3,000 lines, `provider/kubernetes.rs`, `tests/kubernetes_live.rs`), every modified file in the diff, and cross-checked the new transport's assumptions against the `kube` 4.2 source it depends on and the Docker provider it parallels.

## Findings

### 1. Blocking — `spawn_stdio_process` drops the `AttachedProcess`, which aborts kube's I/O pump; stdio sessions die immediately

`lib/components/fabro-sandbox/src/kubernetes.rs:2193-2269`

The function takes the three stream halves out of `attached` (`stdout()`/`stderr()`/`stdin()` all `.take()`), then lets `attached` fall out of scope at function end — nothing stores it. In kube-client 4.2, `AttachedProcess::drop` calls `task.abort()` on the message loop (`kube-client-4.2.0/src/api/remote_command.rs:116-121`), and that task is the pump moving bytes between the WebSocket and the stream halves (it also owns the duplex writers and the stdin reader half).

Consequence: as soon as `spawn_stdio_process` returns, the pump is aborted, the writers are dropped, and the forwarding task (line 2219) sees instant EOF on stdout, then EOF on stderr, then caches `exited(None)`. Concrete trigger: spawn any MCP/ACP stdio process on a Kubernetes sandbox — the caller sees immediate empty-stdout "exit"; `fabro-acp/src/transport.rs` fails with "ACP process exited before protocol completed", and stdin writes go into an aborted pump. Every other exec path in this file keeps `attached` alive and calls `.join()` (lines 646-693, 702-705, 1459-1548) — only the stdio path drops it. The live tests never exercise this method, which is why it wasn't caught. Fix: move `attached` into the forwarding task (or the control struct) so it lives until the streams end.

### 2. High (latent, same function) — sequential stdout→stderr drain can deadlock on kube's 1 KB stream buffers

`lib/components/fabro-sandbox/src/kubernetes.rs:2219-2243`

The forwarding task reads stdout to EOF *before* reading any stderr. kube's exec demux writes stdout and stderr frames into separate bounded `tokio::io::duplex(MAX_BUF_SIZE)` streams where `MAX_BUF_SIZE = 1024` bytes, and `start_message_loop` writes them sequentially from a single task. If the child writes more than 1 KB to stderr while keeping stdout open — the normal MCP/ACP shape (stderr logging, persistent protocol stdout) — the pump blocks in `stderr.write_all`, which also stops stdout delivery, while this task blocks in `stdout_reader.read()`: permanent deadlock. The Docker provider reads a single multiplexed stream (`docker.rs:2098-2117`), so it doesn't have this shape. This is masked today by finding 1, but it will bite as soon as finding 1 is fixed; the fix is a `select!` over both readers (as `kubernetes_exec` at line 664 already does correctly).

### 3. Low — missed-wakeup race in `wait_for_cached_termination` can hang `wait()` forever

`lib/components/fabro-sandbox/src/kubernetes.rs:2703-2710`

The loop checks `cached_termination()` then awaits `notified()` — but `notify_waiters()` only wakes already-registered waiters, and registration happens on first poll of `Notified`. If termination is cached between the check and the poll, the single `notify_waiters` fires before any waiter is registered and `wait()` sleeps forever. The state struct is copied from Docker (`docker.rs:1496-1528`), but Docker's `wait()` is saved by its 1-second `inspect_exec` polling fallback (`docker.rs:1540-1566`); the kubernetes `wait()` (line 2736) has no fallback. The only in-repo caller is partially guarded (`fabro-acp/src/transport.rs:164` wraps one branch in a 500 ms timeout, and the `select` arm at 167 is bounded by the protocol arm), so this is latent, but the generic `StdioProcessHandle::wait()` contract can hang.

### 4. Test bug — live timeout test self-matches and fails even when the kill works

`lib/components/fabro-sandbox/tests/kubernetes_live.rs:182-192`

The kill-verification probe runs `pgrep -f 'sleep 30' >/dev/null; echo $?` through the controlled wrapper. `pgrep -f` excludes only itself — but the probe's own outer wrapper bash has the literal bytes `sleep 30` in its argv (inside the `user_command='…'` assignment), and the inner `setsid bash -c "pgrep -f 'sleep 30' …"` has it too. Both match, `pgrep` exits `0`, `stdout` is `"0"`, and `assert_eq!(check.stdout.trim(), "1")` fails against a real cluster even though the kill succeeded. The Docker equivalent (`tests/docker_streaming.rs:73-76`) deliberately splits the marker (`'fabro_streaming_timeout_''sentinel'`) plus awk filtering to avoid exactly this. The plan names this live file as the manual verification path for the exec/kill transport, so a guaranteed false failure there defeats its purpose.

### Minor (non-blocking)

- `kubernetes.rs:857-869 / 917-926`: download/upload transfers are hard-capped at 30 s. Docker's equivalent (`docker.rs:317-341`) is unbounded. Large-file transfers over a slow cluster will fail where Docker succeeds — a robustness divergence, not a logic error.

## What I verified as correct

The sentinel scheme (`ExecSentinelReader` push/drain/finish arithmetic, last-occurrence anchoring, binary-payload stripping — including the no-trailing-newline and chunk-boundary-split cases), the wrapper script's quoting/`setsid`/pid-file/kill-by-process-group logic and its per-exec nonce (no cross-exec stale-stop contamination), clone flow parity with Docker (including pinned-revision branch enforcement and credential minting), config mapping parity (`skip_clone`, depth, resources), pod/NetworkPolicy manifests, pod-name lowercasing, managed-label verification on reconnect, preflight (`skip_clone = true`, random pod name, cleanup), phase→state mapping and quantity parsing, all cfg-gate widenings, enum/server/config/web/OpenAPI surfaces, and the regenerated TS client (the `pull_request_environment_unsupported` doc change is a legit sync of a stale artifact, already in the yaml on main).

Findings 1 and 2 are the ones I'd fix before this ships; both are contained in `spawn_stdio_process`.