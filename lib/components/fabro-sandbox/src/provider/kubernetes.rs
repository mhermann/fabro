use async_trait::async_trait;
use fabro_types::{SandboxInfo, SandboxProviderKind};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, DeleteParams, ListParams};

use super::{SandboxCreateSpec, SandboxProvider};
use crate::kubernetes::{KubernetesSandbox, connect};
use crate::managed_labels::{self, MANAGED_LABEL, MANAGED_LABEL_VALUE};
use crate::{Sandbox, details};

#[derive(Debug, Clone, Default)]
pub struct KubernetesSandboxProvider;

impl KubernetesSandboxProvider {
    pub fn new() -> Self {
        Self
    }

    /// Probe cluster reachability for diagnostics.
    pub async fn check_cluster() -> crate::Result<String> {
        let (client, _namespace) = connect().await?;
        let version = client
            .apiserver_version()
            .await
            .map_err(|err| crate::Error::context("Failed to reach Kubernetes API server", err))?;
        Ok(format!(
            "{}.{}",
            version.major.trim_end_matches('+'),
            version.minor.trim_end_matches('+')
        ))
    }
}

#[async_trait]
impl SandboxProvider for KubernetesSandboxProvider {
    fn kind(&self) -> SandboxProviderKind {
        SandboxProviderKind::Kubernetes
    }

    async fn list(&self) -> crate::Result<Vec<SandboxInfo>> {
        let (client, namespace) = connect().await?;
        Self::list_with_client(client, &namespace).await
    }

    async fn get(&self, id: &str) -> crate::Result<Option<SandboxInfo>> {
        let (client, namespace) = connect().await?;
        Self::get_with_client(client, &namespace, id).await
    }

    async fn create(&self, spec: SandboxCreateSpec) -> crate::Result<SandboxInfo> {
        let SandboxCreateSpec::Kubernetes {
            config,
            github_app,
            run_id,
            clone_origin_url,
            clone_branch,
        } = spec
        else {
            return Err(crate::Error::message(
                "Kubernetes sandbox provider can only create Kubernetes sandboxes",
            ));
        };

        let sandbox = KubernetesSandbox::new(
            config,
            github_app.as_ref(),
            run_id,
            clone_origin_url,
            clone_branch,
            None,
            None,
        )
        .await?;
        sandbox.initialize().await?;
        let pod_name = sandbox.pod_identifier()?.to_string();
        self.get(&pod_name).await?.ok_or_else(|| {
            crate::Error::message(format!(
                "Kubernetes sandbox '{pod_name}' was created but is not visible in provider inventory"
            ))
        })
    }

    async fn delete(&self, id: &str) -> crate::Result<()> {
        let (client, namespace) = connect().await?;
        Self::delete_with_client(client, &namespace, id).await
    }
}

impl KubernetesSandboxProvider {
    async fn list_with_client(
        client: kube::Client,
        namespace: &str,
    ) -> crate::Result<Vec<SandboxInfo>> {
        let pods: Api<Pod> = Api::namespaced(client, namespace);
        let options =
            ListParams::default().labels(&format!("{MANAGED_LABEL}={MANAGED_LABEL_VALUE}"));
        let pod_list = pods
            .list(&options)
            .await
            .map_err(|err| crate::Error::context("Failed to list Kubernetes pods", err))?;

        // The API-server label selector already restricts to managed pods, so
        // every returned pod maps without a per-pod managed re-check.
        Ok(pod_list
            .iter()
            .map(details::kubernetes::kubernetes_info_from_pod)
            .collect())
    }

    async fn get_with_client(
        client: kube::Client,
        namespace: &str,
        id: &str,
    ) -> crate::Result<Option<SandboxInfo>> {
        let pods: Api<Pod> = Api::namespaced(client, namespace);
        let Some(pod) = pods.get_opt(id).await.map_err(|err| {
            crate::Error::context(format!("Failed to get Kubernetes pod '{id}'"), err)
        })?
        else {
            return Ok(None);
        };
        if !managed_from_pod(&pod) {
            return Ok(None);
        }
        Ok(Some(details::kubernetes::kubernetes_info_from_pod(&pod)))
    }

    async fn delete_with_client(
        client: kube::Client,
        namespace: &str,
        id: &str,
    ) -> crate::Result<()> {
        let pods: Api<Pod> = Api::namespaced(client, namespace);
        let Some(pod) = pods.get_opt(id).await.map_err(|err| {
            crate::Error::context(format!("Failed to get Kubernetes pod '{id}'"), err)
        })?
        else {
            return Ok(());
        };
        if !managed_from_pod(&pod) {
            return Err(crate::Error::message(format!(
                "Refusing to delete Kubernetes pod '{id}' because it is missing label {MANAGED_LABEL}={MANAGED_LABEL_VALUE}"
            )));
        }

        // The per-pod NetworkPolicy carries an ownerReference to the pod, so
        // the API server garbage-collects it together with the pod.
        pods.delete(id, &DeleteParams::default())
            .await
            .map_err(|err| {
                crate::Error::context(format!("Failed to delete Kubernetes pod '{id}'"), err)
            })?;
        Ok(())
    }
}

fn managed_from_pod(pod: &Pod) -> bool {
    // k8s-openapi models metadata labels as a BTreeMap; the shared managed
    // check reads the provider-neutral HashMap shape.
    pod.metadata.labels.as_ref().is_some_and(|labels| {
        managed_labels::is_managed(
            &labels
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        )
    })
}

#[cfg(test)]
mod tests {
    use httpmock::Method::{DELETE, GET};
    use httpmock::MockServer;
    use kube::Config;

    use super::*;

    fn mock_client(server: &MockServer) -> kube::Client {
        install_crypto_provider();
        let uri: http::Uri = server
            .base_url()
            .parse()
            .expect("mock server base url should parse");
        kube::Client::try_from(Config::new(uri)).expect("mock kube client should build")
    }

    /// rustls cannot auto-select a provider when both ring and aws-lc-rs
    /// reach the test tree, so pick one explicitly before building clients.
    fn install_crypto_provider() {
        use rustls::crypto::ring::default_provider;

        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let _ = default_provider().install_default();
        });
    }

    fn managed_pod_json() -> serde_json::Value {
        serde_json::json!({
            "metadata": {
                "name": "fabro-pod",
                "labels": {
                    "sh.fabro.managed": "true",
                    "sh.fabro.sandbox": "fabro-pod"
                }
            },
            "spec": { "containers": [{ "name": "fabro", "image": "buildpack-deps:noble" }] },
            "status": { "phase": "Running" }
        })
    }

    #[tokio::test]
    async fn list_sends_the_managed_label_selector_and_maps_pods() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v1/namespaces/default/pods")
                    .query_param("labelSelector", "sh.fabro.managed=true");
                then.status(200)
                    .header("content-type", "application/json")
                    .json_body(serde_json::json!({
                        "apiVersion": "v1",
                        "kind": "PodList",
                        "items": [managed_pod_json()]
                    }));
            })
            .await;

        let sandboxes =
            KubernetesSandboxProvider::list_with_client(mock_client(&server), "default")
                .await
                .expect("list should succeed");

        assert_eq!(sandboxes.len(), 1);
        assert_eq!(sandboxes[0].provider, SandboxProviderKind::Kubernetes);
        assert_eq!(sandboxes[0].id, "fabro-pod");
        assert_eq!(sandboxes[0].state, fabro_types::SandboxState::Running);
    }

    #[tokio::test]
    async fn get_returns_none_for_missing_pods() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v1/namespaces/default/pods/gone");
                then.status(404)
                    .header("content-type", "application/json")
                    .json_body(serde_json::json!({
                        "kind": "Status",
                        "apiVersion": "v1",
                        "metadata": {},
                        "status": "Failure",
                        "message": "pods \"gone\" not found",
                        "reason": "NotFound",
                        "code": 404
                    }));
            })
            .await;

        let found =
            KubernetesSandboxProvider::get_with_client(mock_client(&server), "default", "gone")
                .await
                .expect("a missing pod should not be an error");
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn get_refuses_unmanaged_pods() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v1/namespaces/default/pods/foreign");
                then.status(200)
                    .header("content-type", "application/json")
                    .json_body(serde_json::json!({
                        "metadata": { "name": "foreign", "labels": {} },
                        "status": { "phase": "Running" }
                    }));
            })
            .await;

        let found =
            KubernetesSandboxProvider::get_with_client(mock_client(&server), "default", "foreign")
                .await
                .expect("an unmanaged pod should not be an error");
        assert!(found.is_none(), "unmanaged pods must stay out of inventory");
    }

    #[tokio::test]
    async fn delete_is_idempotent_for_missing_pods() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v1/namespaces/default/pods/gone");
                then.status(404)
                    .header("content-type", "application/json")
                    .json_body(serde_json::json!({
                        "kind": "Status",
                        "apiVersion": "v1",
                        "metadata": {},
                        "status": "Failure",
                        "message": "pods \"gone\" not found",
                        "reason": "NotFound",
                        "code": 404
                    }));
            })
            .await;

        KubernetesSandboxProvider::delete_with_client(mock_client(&server), "default", "gone")
            .await
            .expect("deleting a missing pod should succeed");
    }

    #[tokio::test]
    async fn delete_refuses_unmanaged_pods() {
        let server = MockServer::start_async().await;
        let delete = server
            .mock_async(|when, then| {
                when.method(DELETE)
                    .path("/api/v1/namespaces/default/pods/foreign");
                then.status(200);
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/api/v1/namespaces/default/pods/foreign");
                then.status(200)
                    .header("content-type", "application/json")
                    .json_body(serde_json::json!({
                        "metadata": { "name": "foreign", "labels": {} },
                        "status": { "phase": "Running" }
                    }));
            })
            .await;

        let error = KubernetesSandboxProvider::delete_with_client(
            mock_client(&server),
            "default",
            "foreign",
        )
        .await
        .expect_err("unmanaged pods must be refused");
        assert!(
            error.to_string().contains("Refusing to delete"),
            "unexpected error: {error}"
        );
        delete.assert_calls_async(0).await;
    }
}
