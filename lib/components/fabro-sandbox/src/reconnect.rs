use std::path::PathBuf;

#[allow(
    unused_imports,
    reason = "Feature-gated branches consume these imports when optional backends are enabled."
)]
use anyhow::{Context, Result, bail};
use fabro_types::{RunId, RunSandboxInstance, SandboxProviderKind};

use crate::SandboxEventCallback;
#[cfg(feature = "daytona")]
use crate::daytona::DaytonaSandbox;
#[cfg(feature = "docker")]
use crate::docker::DockerSandbox;
#[cfg(feature = "kubernetes")]
use crate::kubernetes::KubernetesSandbox;
use crate::local::LocalSandbox;

/// Provider credentials a caller resolves once and hands to every reconnect
/// in a request path.
///
/// `daytona_api_key` is forwarded to the Daytona SDK when the provider is
/// `"daytona"`. `kubernetes_agent_key` is the secret the agent bearer token is
/// derived from when the provider is `"kubernetes"` (the server session
/// secret; only the derived token reaches the pod). Pass `None` fields to fall
/// back to each provider's ambient resolution.
#[derive(Debug, Clone, Default)]
pub struct ReconnectCredentials {
    pub daytona_api_key:      Option<String>,
    pub kubernetes_agent_key: Option<String>,
}

/// Reconnect to a sandbox from a saved record.
pub async fn reconnect(
    record: &RunSandboxInstance,
    credentials: ReconnectCredentials,
) -> Result<Box<dyn crate::Sandbox>> {
    reconnect_for_run(record, credentials, None).await
}

#[allow(
    unused_variables,
    reason = "Feature-gated sandbox backends leave parameters unused on partial builds."
)]
pub async fn reconnect_for_run(
    record: &RunSandboxInstance,
    credentials: ReconnectCredentials,
    run_id: Option<RunId>,
) -> Result<Box<dyn crate::Sandbox>> {
    reconnect_for_run_with_callback(record, credentials, run_id, None).await
}

#[allow(
    unused_variables,
    reason = "Feature-gated sandbox backends leave parameters unused on partial builds."
)]
pub async fn reconnect_for_run_with_callback(
    record: &RunSandboxInstance,
    credentials: ReconnectCredentials,
    run_id: Option<RunId>,
    event_callback: Option<SandboxEventCallback>,
) -> Result<Box<dyn crate::Sandbox>> {
    let runtime = &record.runtime;
    match record.provider {
        SandboxProviderKind::Local => {
            let mut sandbox = LocalSandbox::new(PathBuf::from(&runtime.working_directory));
            if let Some(callback) = event_callback {
                sandbox.set_event_callback(callback);
            }
            Ok(Box::new(sandbox))
        }
        #[cfg(feature = "docker")]
        SandboxProviderKind::Docker => {
            let repo_cloned = runtime
                .repo_cloned
                .context("Docker run sandbox missing repo_cloned metadata")?;
            let mut sandbox = DockerSandbox::reconnect(
                &runtime.id,
                repo_cloned,
                runtime.working_directory.clone(),
                runtime.clone_origin_url.clone(),
                runtime.clone_branch.clone(),
                run_id,
            )
            .await
            .context("Failed to reconnect Docker sandbox")?;
            if let Some(callback) = event_callback {
                sandbox.set_event_callback(callback);
            }
            Ok(Box::new(sandbox))
        }
        #[cfg(not(feature = "docker"))]
        SandboxProviderKind::Docker => bail!("Docker sandbox support is not enabled"),
        #[cfg(feature = "daytona")]
        SandboxProviderKind::Daytona => {
            let repo_cloned = runtime
                .repo_cloned
                .context("Daytona run sandbox missing repo_cloned metadata")?;

            let mut sandbox = DaytonaSandbox::reconnect(
                &runtime.id,
                credentials.daytona_api_key,
                repo_cloned,
                runtime.working_directory.clone(),
                runtime.clone_origin_url.clone(),
                runtime.clone_branch.clone(),
            )
            .await
            .map_err(anyhow::Error::new)?;
            if let Some(callback) = event_callback {
                sandbox.set_event_callback(callback);
            }
            Ok(Box::new(sandbox))
        }
        #[cfg(not(feature = "daytona"))]
        SandboxProviderKind::Daytona => bail!("Daytona sandbox support is not enabled"),
        #[cfg(feature = "kubernetes")]
        SandboxProviderKind::Kubernetes => {
            let repo_cloned = runtime
                .repo_cloned
                .context("Kubernetes run sandbox missing repo_cloned metadata")?;

            let mut sandbox = KubernetesSandbox::reconnect(
                &runtime.id,
                credentials.kubernetes_agent_key,
                repo_cloned,
                runtime.working_directory.clone(),
                runtime.clone_origin_url.clone(),
                runtime.clone_branch.clone(),
                run_id,
            )
            .await
            .map_err(anyhow::Error::new)?;
            if let Some(callback) = event_callback {
                sandbox.set_event_callback(callback);
            }
            Ok(Box::new(sandbox))
        }
        #[cfg(not(feature = "kubernetes"))]
        SandboxProviderKind::Kubernetes => bail!("Kubernetes sandbox support is not enabled"),
    }
}
