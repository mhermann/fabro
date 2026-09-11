use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result as AnyResult};
use async_trait::async_trait;
use fabro_agent::cli::{
    OutputFormat, diagnostic_client_options, run_with_args_and_client_and_catalog,
    run_with_args_and_source_and_catalog,
};
use fabro_llm::ErrorKind;
use fabro_llm::gateway::{GatewayAdapter, GatewayError, GatewayTransport};
use fabro_llm::lithos_catalog::Catalog;
use fabro_mcp::config::McpServerSettings;
use fabro_types::settings::cli::OutputFormat as SettingsOutputFormat;
use fabro_types::settings::run::ResolvedMcpEntry;
use fabro_util::exit::{self, ErrorExt, ExitClass};
use lithos_llm::catalog::ProviderId;

use crate::args::ExecArgs;
use crate::command_context::CommandContext;
#[cfg(feature = "sleep_inhibitor")]
use crate::sleep_inhibitor;
use crate::{server_client, user_config};

/// Posts completions to a Fabro server through the authenticated CLI client.
struct ServerCompletionTransport {
    client:   server_client::Client,
    base_url: String,
}

impl ServerCompletionTransport {
    fn new(client: server_client::Client) -> Self {
        let base_url = client.base_url();
        Self { client, base_url }
    }
}

#[async_trait]
impl GatewayTransport for ServerCompletionTransport {
    async fn post_completion(
        &self,
        body: serde_json::Value,
    ) -> Result<fabro_http::Response, GatewayError> {
        let url = format!("{}/api/v1/completions", self.base_url);
        let response = self
            .client
            .send_http_response(|http_client| {
                let body = body.clone();
                let url = url.clone();
                async move { http_client.post(url).json(&body).send().await }
            })
            .await
            .map_err(|err| GatewayError::Transport {
                auth:    exit::exit_class_for(&err) == Some(ExitClass::AuthRequired),
                message: err.to_string(),
            })?;
        response.map_err(|failure| GatewayError::Status {
            status:  failure.status.as_u16(),
            headers: failure.headers,
            body:    failure.body,
        })
    }
}

fn classify_server_agent_auth(err: anyhow::Error) -> anyhow::Error {
    let is_auth = err.chain().any(|cause| {
        cause
            .downcast_ref::<fabro_agent::Error>()
            .is_some_and(|error| {
                matches!(
                    error,
                    fabro_agent::Error::Llm(llm) if llm.kind() == ErrorKind::Authentication
                )
            })
    });
    if is_auth {
        err.classify(ExitClass::AuthRequired)
    } else {
        err
    }
}

fn run_mcp_servers_for_exec(
    mcps: &HashMap<String, ResolvedMcpEntry>,
) -> AnyResult<Vec<McpServerSettings>> {
    mcps.iter()
        .map(|(key, entry)| match entry {
            ResolvedMcpEntry::Resolved(server) => Ok(server.clone()),
            ResolvedMcpEntry::Reference(reference) => {
                anyhow::bail!(
                    "fabro exec cannot resolve run.agent.mcps.{key} catalog reference \
                     (id `{}`); define an inline server under [cli.exec.agent.mcps.{key}] or \
                     remove the run-level reference",
                    reference.id
                );
            }
        })
        .collect()
}

pub(crate) async fn execute(mut args: ExecArgs, ctx: &CommandContext) -> AnyResult<()> {
    let cli = &ctx.user_settings().cli;
    #[cfg(feature = "sleep_inhibitor")]
    let _sleep_guard = sleep_inhibitor::guard(cli.exec.prevent_idle_sleep);
    let provider_str = cli.exec.model.provider.as_deref();
    let model_str = cli.exec.model.name.as_deref();
    let permissions = cli.exec.agent.permissions;
    let output_format = Some(match cli.output.format {
        SettingsOutputFormat::Text => OutputFormat::Text,
        SettingsOutputFormat::Json => OutputFormat::Json,
    });
    args.agent
        .apply_cli_defaults(provider_str, model_str, permissions, output_format);
    let server_target = user_config::exec_server_target(&args.server)?;
    // v2 MCPs live under `cli.exec.agent.mcps` (owner-specific) or
    // `run.agent.mcps`. For `fabro exec` we use the cli.exec path, falling
    // back to run.agent.mcps if unset.
    let mcp_servers: Vec<McpServerSettings> = match cli.exec.agent.mcps.as_ref() {
        Some(mcps) => mcps.values().cloned().collect(),
        None => ctx
            .run_settings()
            .ok()
            .map(|settings| run_mcp_servers_for_exec(&settings.agent.mcps))
            .transpose()?
            .unwrap_or_default(),
    };
    // Fully validate MCP transport config at the exec boundary. `fabro exec`
    // has no server vault, so secret and unsupported tokens fail instead of
    // reaching the transport.
    let mcp_servers = mcp_servers
        .into_iter()
        .map(|settings| {
            settings
                .resolve_transport_secrets(|_| None)
                .with_context(|| format!("failed to resolve MCP server {:?}", settings.name))
        })
        .collect::<AnyResult<Vec<_>>>()?;
    if let Some(target) = server_target {
        tracing::info!(transport = "server", "Agent session starting");
        let provider_name = args
            .agent
            .provider
            .clone()
            .unwrap_or_else(|| "anthropic".to_string());
        let catalog = ctx.catalog()?;
        let provider_id = catalog.enabled_provider(&provider_name).map_or_else(
            || ProviderId::new(provider_name.as_str()),
            |provider| provider.id().clone(),
        );
        let server_client = server_client::connect_server_target(&target).await?;
        let adapter = Arc::new(GatewayAdapter::new(Box::new(
            ServerCompletionTransport::new(server_client),
        )));
        // The server inlines attachments and is the billing authority, so the
        // local client only routes and reports diagnostics.
        let mut options = diagnostic_client_options(&args.agent);
        options.inline_attachments = false;
        let client = fabro_llm::build_offline_client(
            Catalog::clone(&catalog),
            options.with_adapter(provider_id, adapter),
        )
        .context("Failed to register fabro server adapter")?
        .client;
        run_with_args_and_client_and_catalog(args.agent, client, mcp_servers, catalog)
            .await
            .map_err(classify_server_agent_auth)?;
    } else {
        tracing::info!(transport = "direct", "Agent session starting");
        let llm_source = ctx.llm_source().await?;
        let catalog = ctx.catalog()?;
        run_with_args_and_source_and_catalog(args.agent, llm_source, mcp_servers, catalog).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fabro_types::settings::run::{McpServerRef, McpServerSettings, ResolvedMcpEntry};

    use super::run_mcp_servers_for_exec;

    #[test]
    fn run_mcp_servers_for_exec_rejects_catalog_references() {
        let err = run_mcp_servers_for_exec(&HashMap::from([(
            "sentry".to_string(),
            ResolvedMcpEntry::Reference(McpServerRef {
                id:      "catalog/sentry".to_string(),
                enabled: None,
            }),
        )]))
        .expect_err("fabro exec should reject unresolved run-level MCP references");

        assert!(
            err.to_string()
                .contains("fabro exec cannot resolve run.agent.mcps.sentry catalog reference"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn run_mcp_servers_for_exec_keeps_resolved_servers() {
        let servers = run_mcp_servers_for_exec(&HashMap::from([(
            "inline".to_string(),
            ResolvedMcpEntry::Resolved(McpServerSettings {
                name: "inline".to_string(),
                ..McpServerSettings::default()
            }),
        )]))
        .expect("resolved inline server should be usable by fabro exec");

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "inline");
    }
}
