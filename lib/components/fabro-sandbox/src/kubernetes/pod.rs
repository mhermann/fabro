//! Pod-manifest construction for Kubernetes sandboxes.
//!
//! Pure builders only: every value here is derived from
//! [`KubernetesSandboxOptions`] plus the run identity, so the manifests are
//! unit-testable without a cluster. Transport (create/watch/exec) lives in
//! `mod.rs` and `agent_client.rs`.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{
    Container, ContainerPort, EmptyDirVolumeSource, EnvVar, ExecAction, Pod, PodSpec, Probe,
    ResourceRequirements, SecurityContext, TCPSocketAction, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

use super::{KubernetesSandboxOptions, REPOS_ROOT, WORKING_DIRECTORY};
use crate::managed_labels::{self, MANAGED_LABEL, MANAGED_LABEL_VALUE, RUN_ID_LABEL};

pub(crate) const POD_NAME_PREFIX: &str = "fabro-run";

/// Directory the init container installs the agent binary into. Shared with
/// the main container through an `emptyDir` volume.
pub(crate) const AGENT_BIN_VOLUME: &str = "fabro-agent-bin";
pub(crate) const AGENT_BIN_DIR: &str = "/fabro";
pub(crate) const AGENT_BIN_PATH: &str = "/fabro/bin/fabro-sandbox-agent";

/// Path of the agent binary inside the published agent image.
pub(crate) const AGENT_IMAGE_BINARY: &str = "/fabro-sandbox-agent";

/// Environment variable carrying the derived agent bearer token.
pub(crate) const AGENT_TOKEN_ENV: &str = "FABRO_KUBERNETES_AGENT_TOKEN";

pub(crate) const WORKSPACE_VOLUME: &str = "workspace";
pub(crate) const REPOS_VOLUME: &str = "repos";

/// Compose the deterministic pod name for a run sandbox.
///
/// RunIds serialize as uppercase ULIDs; Kubernetes pod names must be
/// lowercase DNS-1123 labels, so the identifier is lowercased here.
pub(crate) fn pod_name(run_id: &fabro_types::RunId) -> String {
    format!("{POD_NAME_PREFIX}-{}", run_id.to_string().to_lowercase())
}

/// Compose a unique pod name for provider-managed sandboxes without a run
/// identity (preflight probes). Segment length keeps the total within the
/// 63-character DNS-1123 label limit.
pub(crate) fn anonymous_pod_name() -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!("{POD_NAME_PREFIX}-probe-{}", &suffix[..12])
}

/// Build the sandbox pod for `options`.
///
/// One init container installs the Fabro sandbox agent into the shared
/// `emptyDir` and prepares the workspace directories; the main container runs
/// the user's image with its command overridden to serve the agent protocol.
pub(crate) fn build_pod(
    name: &str,
    options: &KubernetesSandboxOptions,
    run_id: Option<&fabro_types::RunId>,
    agent_token: &str,
) -> Pod {
    let labels = pod_labels(options, run_id);
    let volumes = vec![
        Volume {
            name: AGENT_BIN_VOLUME.to_string(),
            empty_dir: Some(EmptyDirVolumeSource::default()),
            ..Volume::default()
        },
        Volume {
            name: WORKSPACE_VOLUME.to_string(),
            empty_dir: Some(EmptyDirVolumeSource::default()),
            ..Volume::default()
        },
        Volume {
            name: REPOS_VOLUME.to_string(),
            empty_dir: Some(EmptyDirVolumeSource::default()),
            ..Volume::default()
        },
    ];

    let init_container = Container {
        name: "fabro-agent-install".to_string(),
        image: Some(options.agent_image.clone()),
        command: Some(install_command()),
        volume_mounts: Some(vec![
            volume_mount(AGENT_BIN_VOLUME, AGENT_BIN_DIR),
            volume_mount(WORKSPACE_VOLUME, WORKING_DIRECTORY),
            volume_mount(REPOS_VOLUME, REPOS_ROOT),
        ]),
        ..Container::default()
    };

    let mut env = vec![
        EnvVar {
            name: AGENT_TOKEN_ENV.to_string(),
            value: Some(agent_token.to_string()),
            ..EnvVar::default()
        },
        EnvVar {
            name: "FABRO_SANDBOX_PORT".to_string(),
            value: Some(options.agent_port.to_string()),
            ..EnvVar::default()
        },
    ];
    env.extend(options.env_vars.iter().map(|entry| {
        let (name, value) = entry.split_once('=').unwrap_or((entry, ""));
        EnvVar {
            name: name.to_string(),
            value: Some(value.to_string()),
            ..EnvVar::default()
        }
    }));

    let main_container = Container {
        name: "sandbox".to_string(),
        image: Some(options.image.clone()),
        command: Some(vec![
            AGENT_BIN_PATH.to_string(),
            "serve".to_string(),
            "--port".to_string(),
            options.agent_port.to_string(),
        ]),
        env: Some(env),
        ports: Some(vec![ContainerPort {
            name: Some("fabro-agent".to_string()),
            container_port: i32::from(options.agent_port),
            protocol: Some("TCP".to_string()),
            ..ContainerPort::default()
        }]),
        // The agent is PID 1 and shares the kernel with untrusted run
        // processes; privilege escalation adds nothing the sandbox needs.
        security_context: Some(SecurityContext {
            allow_privilege_escalation: Some(false),
            ..SecurityContext::default()
        }),
        resources: Some(ResourceRequirements {
            limits: Some(resource_quantities(options)),
            requests: Some(resource_quantities(options)),
            ..ResourceRequirements::default()
        }),
        startup_probe: Some(Probe {
            exec: Some(ExecAction {
                command: Some(vec![
                    AGENT_BIN_PATH.to_string(),
                    "healthz".to_string(),
                    "--port".to_string(),
                    options.agent_port.to_string(),
                ]),
            }),
            ..default_tcp_probe(options.agent_port)
        }),
        volume_mounts: Some(vec![
            volume_mount(AGENT_BIN_VOLUME, AGENT_BIN_DIR),
            volume_mount(WORKSPACE_VOLUME, WORKING_DIRECTORY),
            volume_mount(REPOS_VOLUME, REPOS_ROOT),
        ]),
        ..Container::default()
    };

    Pod {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            labels: Some(labels),
            ..ObjectMeta::default()
        },
        spec:     Some(PodSpec {
            restart_policy: Some("Never".to_string()),
            init_containers: Some(vec![init_container]),
            containers: vec![main_container],
            volumes: Some(volumes),
            ..PodSpec::default()
        }),
        status:   None,
    }
}

/// Pod state for an already-created sandbox, used by reconnect validation.
pub(crate) fn pod_phase(pod: &Pod) -> Option<&str> {
    pod.status
        .as_ref()
        .and_then(|status| status.phase.as_deref())
}

fn install_command() -> Vec<String> {
    vec![
        AGENT_IMAGE_BINARY.to_string(),
        "self-install".to_string(),
        AGENT_BIN_PATH.to_string(),
        // The image's default user is unknown, so the prepared directories
        // must be writable by any uid the main container runs as.
        "--prepare-dir".to_string(),
        WORKING_DIRECTORY.to_string(),
        "--prepare-dir".to_string(),
        REPOS_ROOT.to_string(),
    ]
}

fn default_tcp_probe(port: u16) -> Probe {
    Probe {
        tcp_socket: Some(TCPSocketAction {
            port: IntOrString::Int(i32::from(port)),
            ..TCPSocketAction::default()
        }),
        period_seconds: Some(2),
        failure_threshold: Some(150),
        ..Probe::default()
    }
}

fn volume_mount(volume: &str, mount_path: &str) -> VolumeMount {
    VolumeMount {
        name: volume.to_string(),
        mount_path: mount_path.to_string(),
        ..VolumeMount::default()
    }
}

fn resource_quantities(options: &KubernetesSandboxOptions) -> BTreeMap<String, Quantity> {
    let mut quantities = BTreeMap::new();
    if let Some(cpu) = options.cpu {
        quantities.insert(
            "cpu".to_string(),
            Quantity(format!("{}m", cpu.saturating_mul(1_000))),
        );
    }
    if let Some(memory) = options.memory_bytes {
        quantities.insert("memory".to_string(), Quantity(format_kibi(memory)));
    }
    if let Some(disk) = options.disk_bytes {
        quantities.insert("ephemeral-storage".to_string(), Quantity(format_kibi(disk)));
    }
    quantities
}

/// Render a byte count as a Kubernetes binary-suffix quantity. Quantities are
/// integer strings with `Ki` units so no rounding can lose capacity.
fn format_kibi(bytes: i64) -> String {
    let kibi = bytes.max(0) / 1024;
    format!("{kibi}Ki")
}

fn pod_labels(
    options: &KubernetesSandboxOptions,
    run_id: Option<&fabro_types::RunId>,
) -> BTreeMap<String, String> {
    managed_labels::merge_for_run(options.labels.as_ref(), run_id)
        .into_iter()
        .collect()
}

/// True when the pod's labels carry the Fabro managed sentinel.
///
/// Kubernetes label maps are `BTreeMap`s (the Docker inventory uses
/// `HashMap`s), so the sentinel check is inlined here instead of reusing
/// `managed_labels::is_managed`.
pub(crate) fn managed_from_pod(pod: &Pod) -> bool {
    pod.metadata.labels.as_ref().is_some_and(|labels| {
        labels.get(MANAGED_LABEL).map(String::as_str) == Some(MANAGED_LABEL_VALUE)
    })
}

/// The run identity recorded on the pod's labels, when present.
pub(crate) fn run_id_from_pod(pod: &Pod) -> Option<String> {
    pod.metadata
        .labels
        .as_ref()
        .and_then(|labels| labels.get(RUN_ID_LABEL).cloned())
}

#[cfg(test)]
mod tests {
    use KubernetesSandboxOptions;
    use fabro_types::RunId;

    use super::*;

    fn options() -> KubernetesSandboxOptions {
        KubernetesSandboxOptions {
            image: "buildpack-deps:noble".to_string(),
            ..KubernetesSandboxOptions::default()
        }
    }

    #[test]
    fn pod_name_lowercases_run_id_and_stays_a_dns_label() {
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        let name = pod_name(&run_id);

        assert_eq!(name, "fabro-run-01hy0000000000000000000000");
        assert!(name.len() <= 63);
        assert!(
            name.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        );
    }

    #[test]
    fn manifest_overrides_command_with_agent_and_mounts_volumes() {
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        let pod = build_pod(&pod_name(&run_id), &options(), Some(&run_id), "token");

        let spec = pod.spec.expect("pod spec");
        assert_eq!(spec.restart_policy.as_deref(), Some("Never"));
        assert_eq!(spec.init_containers.as_ref().map(Vec::len), Some(1));

        let container = &spec.containers[0];
        assert_eq!(container.image.as_deref(), Some("buildpack-deps:noble"));
        let command = container.command.as_ref().expect("agent command");
        assert_eq!(command[0], AGENT_BIN_PATH);
        assert_eq!(command[1], "serve");

        let mounts = container.volume_mounts.as_ref().expect("volume mounts");
        let paths: Vec<&str> = mounts
            .iter()
            .map(|mount| mount.mount_path.as_str())
            .collect();
        assert_eq!(paths, vec!["/fabro", WORKING_DIRECTORY, REPOS_ROOT]);
    }

    #[test]
    fn manifest_carries_managed_labels_and_derived_env() {
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        let mut options = options();
        options.env_vars = vec!["TEAM=platform".to_string()];
        let pod = build_pod("pod", &options, Some(&run_id), "agent-token");

        let labels = pod.metadata.labels.as_ref().unwrap();
        assert_eq!(
            labels.get(MANAGED_LABEL).map(String::as_str),
            Some(MANAGED_LABEL_VALUE)
        );
        assert_eq!(
            labels.get(RUN_ID_LABEL).map(String::as_str),
            Some("01HY0000000000000000000000")
        );

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let env = container.env.as_ref().unwrap();
        assert!(
            env.iter()
                .any(|var| var.name == AGENT_TOKEN_ENV
                    && var.value.as_deref() == Some("agent-token"))
        );
        assert!(
            env.iter()
                .any(|var| var.name == "TEAM" && var.value.as_deref() == Some("platform"))
        );
    }

    #[test]
    fn manifest_maps_resources_onto_requests_and_limits() {
        let mut options = options();
        options.cpu = Some(2);
        options.memory_bytes = Some(4 * 1024 * 1024 * 1024);
        options.disk_bytes = Some(10 * 1024 * 1024 * 1024);
        let pod = build_pod("pod", &options, None, "token");

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let resources = container.resources.as_ref().unwrap();
        let requests = resources.requests.as_ref().unwrap();
        let limits = resources.limits.as_ref().unwrap();
        assert_eq!(requests.get("cpu").map(|q| q.0.as_str()), Some("2000m"));
        assert_eq!(
            requests.get("memory").map(|q| q.0.as_str()),
            Some("4194304Ki")
        );
        assert_eq!(
            requests.get("ephemeral-storage").map(|q| q.0.as_str()),
            Some("10485760Ki")
        );
        assert_eq!(requests, limits);
    }

    #[test]
    fn managed_detection_reads_pod_labels() {
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        let pod = build_pod("pod", &options(), Some(&run_id), "token");
        assert!(managed_from_pod(&pod));
        assert_eq!(
            run_id_from_pod(&pod).as_deref(),
            Some("01HY0000000000000000000000")
        );

        let unmanaged = Pod::default();
        assert!(!managed_from_pod(&unmanaged));
    }

    #[test]
    fn install_command_prepares_writable_workspace_directories() {
        let command = install_command();
        assert_eq!(command[0], AGENT_IMAGE_BINARY);
        assert_eq!(command[1], "self-install");
        assert!(
            command
                .windows(2)
                .any(|pair| pair[0] == "--prepare-dir" && pair[1] == WORKING_DIRECTORY)
        );
    }
}
