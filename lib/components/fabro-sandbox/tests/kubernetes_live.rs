//! Live Kubernetes cluster tests.
//!
//! Mirrors `docker_streaming.rs`: transport behavior cannot be exercised by
//! unit tests, so these run against a real cluster and are skipped unless
//! explicitly selected. Connection uses kube-standard inference, so point your
//! kubeconfig context at a namespace you can create pods in, for example with
//! kind:
//!
//! ```bash
//! kind create cluster
//! kubectl create namespace fabro-test
//! kubectl config set-context --current --namespace=fabro-test
//! cargo nextest run -p fabro-sandbox --features kubernetes \
//!   --profile e2e --run-ignored only -E 'test(kubernetes_live)'
//! ```
//!
//! The namespace needs RBAC for `pods` (create/get/list/delete), `pods/exec`
//! (create), and `networkpolicies` (create/delete); cluster-admin covers it.
#![cfg(feature = "kubernetes")]

use std::time::Duration;

use fabro_sandbox::{ExecStreamingRequest, KubernetesSandbox, KubernetesSandboxOptions, Sandbox};
use fabro_types::{CommandTermination, RunId};
use tokio::time;

async fn live_sandbox(run_id: RunId) -> Option<KubernetesSandbox> {
    let Ok(sandbox) = KubernetesSandbox::new(
        KubernetesSandboxOptions::default(),
        None,
        Some(run_id),
        None,
        None,
        None,
        None,
    )
    .await
    else {
        tracing::warn!("no reachable Kubernetes cluster — see the module docs for the recipe");
        return None;
    };
    Some(sandbox)
}

#[tokio::test]
#[ignore = "requires a real Kubernetes cluster; run explicitly when changing the kubernetes provider"]
async fn kubernetes_live_initialize_probes_and_stop_deletes_the_pod() {
    let run_id: RunId = "01HYK8ST1000000000000000000".parse().unwrap();
    let Some(sandbox) = live_sandbox(run_id).await else {
        return;
    };

    sandbox
        .initialize()
        .await
        .expect("initialize should create the pod and stay ready");
    // The pod name must be the lowercased run id: the API server rejects
    // mixed-case names.
    assert_eq!(
        sandbox.sandbox_info(),
        format!("fabro-{}", run_id.to_string().to_lowercase())
    );

    let probe = sandbox
        .exec_command("echo fabro-live", 10_000, None, None, None)
        .await
        .expect("exec should run");
    assert!(probe.is_success(), "exec failed: {}", probe.stderr);
    assert_eq!(probe.stdout.trim(), "fabro-live");

    // stop() deletes the pod, so exec afterwards must fail.
    sandbox.stop().await.expect("stop should delete the pod");
    assert!(
        sandbox
            .exec_command("true", 10_000, None, None, None)
            .await
            .is_err(),
        "exec must fail once the pod is deleted"
    );
}

#[tokio::test]
#[ignore = "requires a real Kubernetes cluster; run explicitly when changing the kubernetes provider"]
async fn kubernetes_live_streaming_exec_reports_exit_codes() {
    let run_id: RunId = "01HYK8ST2000000000000000000".parse().unwrap();
    let Some(sandbox) = live_sandbox(run_id).await else {
        return;
    };
    sandbox
        .initialize()
        .await
        .expect("initialize should create the pod");

    let result = sandbox
        .exec_command_streaming(ExecStreamingRequest {
            timeout_ms: Some(10_000),
            ..ExecStreamingRequest::new("echo out-line && echo err-line 1>&2; exit 3")
        })
        .await
        .expect("streaming exec should run");
    assert_eq!(result.result.exit_code, Some(3));
    assert!(
        result.result.stdout.contains("out-line"),
        "stdout: {}",
        result.result.stdout
    );
    assert!(
        result.result.stderr.contains("err-line"),
        "stderr: {}",
        result.result.stderr
    );
    assert!(result.live_streaming);

    sandbox.stop().await.expect("stop should delete the pod");
}

#[tokio::test]
#[ignore = "requires a real Kubernetes cluster; run explicitly when changing the kubernetes provider"]
async fn kubernetes_live_tar_round_trip_and_reconnect() {
    let run_id: RunId = "01HYK8ST3000000000000000000".parse().unwrap();
    let Some(sandbox) = live_sandbox(run_id).await else {
        return;
    };
    sandbox
        .initialize()
        .await
        .expect("initialize should create the pod");

    sandbox
        .write_file("/tmp/fabro-live.txt", "round trip")
        .await
        .expect("write should upload the file");
    let read = sandbox
        .read_file("/tmp/fabro-live.txt", None, None)
        .await
        .expect("read should download the file");
    assert!(read.contains("round trip"), "read: {read}");

    let pod_name = sandbox.sandbox_info();
    let working_directory = sandbox.working_directory().to_string();
    drop(sandbox);

    let reconnected = KubernetesSandbox::reconnect(
        &pod_name,
        false,
        working_directory,
        None,
        None,
        Some(run_id),
    )
    .await
    .expect("reconnect should validate the managed pod");
    assert_eq!(reconnected.sandbox_info(), pod_name);
    reconnected
        .stop()
        .await
        .expect("stop should delete the pod");
}

#[tokio::test]
#[ignore = "requires a real Kubernetes cluster; run explicitly when changing the kubernetes provider"]
async fn kubernetes_live_timed_out_exec_kills_the_command() {
    let run_id: RunId = "01HYK8ST4000000000000000000".parse().unwrap();
    let Some(sandbox) = live_sandbox(run_id).await else {
        return;
    };
    sandbox
        .initialize()
        .await
        .expect("initialize should create the pod");

    let result = sandbox
        .exec_command("sleep 30", 1_000, None, None, None)
        .await
        .expect("exec should return");
    assert_eq!(result.termination, CommandTermination::TimedOut);
    assert_eq!(result.exit_code, None);

    // The kill must reach the wrapped shell: a follow-up exec observes the
    // sandbox healthy and the sleep gone from the process list.
    time::sleep(Duration::from_secs(2)).await;
    let check = sandbox
        .exec_command(
            "pgrep -f 'sleep 30' >/dev/null; echo $?",
            10_000,
            None,
            None,
            None,
        )
        .await
        .expect("follow-up exec should run");
    assert_eq!(check.stdout.trim(), "1", "the timed-out sleep must be gone");

    sandbox.stop().await.expect("stop should delete the pod");
}
