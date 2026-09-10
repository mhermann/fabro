use async_trait::async_trait;
use fabro_types::{SandboxInfo, SandboxProviderKind};
use k8s_openapi::api::core::v1::Pod;
use kube::api::DeleteParams;

use super::{SandboxCreateSpec, SandboxProvider};
use crate::kubernetes::pod::managed_from_pod;
use crate::kubernetes::{self, KubernetesSandbox};
use crate::managed_labels::{MANAGED_LABEL, MANAGED_LABEL_VALUE};
use crate::{Sandbox, details};

type PodApi = kube::Api<Pod>;

/// Inventory provider for Fabro-managed Kubernetes sandbox pods.
///
/// The cluster is resolved from the ambient environment; pods are listed
/// through the managed label selector so only Fabro-created pods are ever
/// reported or deleted.
#[derive(Clone, Default)]
pub struct KubernetesSandboxProvider {
    namespace:       Option<String>,
    agent_token_key: Option<String>,
    client:          Option<kube::Client>,
}

impl KubernetesSandboxProvider {
    /// Build a provider that resolves the ambient kube client per call.
    pub fn new(namespace: Option<String>, agent_token_key: Option<String>) -> Self {
        Self {
            namespace,
            agent_token_key,
            client: None,
        }
    }

    /// Build a provider around an already-resolved client (the server
    /// resolves the cluster once during startup).
    pub fn with_client(
        namespace: Option<String>,
        agent_token_key: Option<String>,
        client: kube::Client,
    ) -> Self {
        Self {
            namespace,
            agent_token_key,
            client: Some(client),
        }
    }

    async fn pods(&self) -> crate::Result<PodApi> {
        let client = match self.client.as_ref() {
            Some(client) => client.clone(),
            None => kube::Client::try_default()
                .await
                .map_err(crate::Error::kubernetes_connect)?,
        };
        let namespace = self
            .namespace
            .clone()
            .unwrap_or_else(|| "default".to_string());
        Ok(kube::Api::namespaced(client, &namespace))
    }
}

#[async_trait]
impl SandboxProvider for KubernetesSandboxProvider {
    fn kind(&self) -> SandboxProviderKind {
        SandboxProviderKind::Kubernetes
    }

    async fn list(&self) -> crate::Result<Vec<SandboxInfo>> {
        use kube::api::ListParams;
        let pods = self.pods().await?;
        // Server-side label filter restricts the result to managed pods, so
        // the per-pod managed re-check is unnecessary.
        let list = pods
            .list(&ListParams::default().labels(&format!("{MANAGED_LABEL}={MANAGED_LABEL_VALUE}")))
            .await
            .map_err(|err| crate::Error::kubernetes_api("pod list", err))?;
        Ok(list
            .iter()
            .map(details::kubernetes::kubernetes_info_from_pod)
            .collect())
    }

    async fn get(&self, id: &str) -> crate::Result<Option<SandboxInfo>> {
        let pods = self.pods().await?;
        match pods.get(id).await {
            Ok(pod) => {
                if !managed_from_pod(&pod) {
                    return Ok(None);
                }
                Ok(Some(details::kubernetes::kubernetes_info_from_pod(&pod)))
            }
            Err(err) if kubernetes::kubernetes_not_found(&err) => Ok(None),
            Err(err) => Err(crate::Error::context(
                format!("Failed to get Kubernetes pod '{id}'"),
                err,
            )),
        }
    }

    async fn create(&self, spec: SandboxCreateSpec) -> crate::Result<SandboxInfo> {
        let SandboxCreateSpec::Kubernetes {
            config,
            github_app,
            run_id,
            clone_origin_url,
            clone_branch,
            agent_token_key,
        } = spec
        else {
            return Err(crate::Error::message(
                "Kubernetes sandbox provider can only create Kubernetes sandboxes",
            ));
        };

        let sandbox = KubernetesSandbox::new(
            config.as_ref().clone(),
            github_app.as_ref(),
            run_id,
            clone_origin_url,
            clone_branch,
            None,
            None,
            agent_token_key.or_else(|| self.agent_token_key.clone()),
        )
        .await?;
        sandbox.initialize().await?;
        let pod_name = sandbox.sandbox_info();
        self.get(&pod_name).await?.ok_or_else(|| {
            crate::Error::message(format!(
                "Kubernetes sandbox '{pod_name}' was created but is not visible in provider \
                 inventory"
            ))
        })
    }

    async fn delete(&self, id: &str) -> crate::Result<()> {
        let pods = self.pods().await?;
        let pod = match pods.get(id).await {
            Ok(pod) => pod,
            Err(err) if kubernetes::kubernetes_not_found(&err) => return Ok(()),
            Err(err) => {
                return Err(crate::Error::context(
                    format!("Failed to get Kubernetes pod '{id}' before delete"),
                    err,
                ));
            }
        };
        if !managed_from_pod(&pod) {
            return Err(crate::Error::message(format!(
                "Refusing to delete Kubernetes pod '{id}' because it is missing label \
                 {MANAGED_LABEL}={MANAGED_LABEL_VALUE}"
            )));
        }

        pods.delete(id, &DeleteParams::default())
            .await
            .map_err(|err| {
                crate::Error::context(format!("Failed to delete Kubernetes pod '{id}'"), err)
            })?;
        Ok(())
    }
}
