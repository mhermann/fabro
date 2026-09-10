#[expect(
    clippy::disallowed_types,
    reason = "CLI entry point writes to stdout/stderr; blocking std::io::Write is intentional and \
              scoped to the CLI binary, not to any library code used by Tokio services"
)]
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use clap::{Args, Parser};
use fabro_auth::SqlVaultCredentialSource;
use fabro_config::Storage;
use fabro_config::user::default_storage_dir;
use fabro_llm::credentials::CredentialProvider;
use fabro_llm::lithos_catalog::{Catalog, CatalogProvider};
use fabro_llm::middleware::{Call, Middleware, Next, Output};
use fabro_llm::{Client, ClientOptions, Error as LlmError, catalog};
use fabro_mcp::config::McpServerSettings;
use fabro_static::EnvVars;
use fabro_types::AgentProfileKind;
use fabro_util::terminal::Styles;
use fabro_vault::SecretStore;
use lithos_llm::catalog::{ModelHandle, ModelId, ProviderId};
use tokio::io::{AsyncWriteExt, stdout};
use tokio::signal;

use crate::config::{ToolApprovalAdapter, ToolApprovalFn, ToolHookCallback, ToolSecrets};
use crate::error::InterruptReason;
use crate::subagent::{SessionFactory, SubAgentSupervisor};
use crate::tool_permissions::{is_auto_approved, tool_category};
use crate::tools::WebFetchSummarizer;
use crate::{
    AgentEvent, AgentProfile, AgentProfileBuilder, LocalSandbox, Message, Sandbox, Session,
    SessionOptions, SessionShutdownReason,
};

#[expect(
    clippy::disallowed_methods,
    reason = "Standalone agent CLI explicitly passes search process-env credentials into tool configuration."
)]
fn cli_tool_secrets() -> ToolSecrets {
    ToolSecrets {
        searxng_url:          std::env::var(EnvVars::SEARXNG_URL).ok(),
        brave_search_api_key: std::env::var(EnvVars::BRAVE_SEARCH_API_KEY).ok(),
        venice_api_key:       std::env::var(EnvVars::VENICE_API_KEY).ok(),
    }
}

/// Public arguments for the agent command, usable from an external CLI.
#[derive(Args)]
pub struct AgentArgs {
    /// Task prompt
    pub prompt: String,

    /// LLM provider (built-in or configured provider ID)
    #[arg(long)]
    pub provider: Option<String>,

    /// Model name (defaults per provider)
    #[arg(long)]
    pub model: Option<String>,

    /// Permission level for tool execution
    #[arg(long, value_enum)]
    pub permissions: Option<PermissionLevel>,

    /// Skip interactive prompts; deny tools outside permission level
    #[arg(long)]
    pub auto_approve: bool,

    /// Print LLM request/response debug info to stderr
    #[arg(long)]
    pub debug: bool,

    /// Print full LLM request/response JSON to stderr
    #[arg(long)]
    pub verbose: bool,

    /// Directory containing skill files (overrides default discovery)
    #[arg(long)]
    pub skills_dir: Option<String>,

    /// Output format (text for human-readable, json for NDJSON event stream)
    #[arg(long, value_enum)]
    pub output_format: Option<OutputFormat>,
}

#[derive(Parser)]
#[command(name = "fabro-agent")]
struct Cli {
    #[command(flatten)]
    args: AgentArgs,
}

/// Output format for the `fabro exec` / agent CLI.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Text,
    Json,
}

pub use fabro_types::{AgentToolCategory, PermissionLevel};

impl AgentArgs {
    /// Fill `None` fields from settings.toml values, then hardcoded defaults.
    pub fn apply_cli_defaults(
        &mut self,
        provider: Option<&str>,
        model: Option<&str>,
        permissions: Option<PermissionLevel>,
        output_format: Option<OutputFormat>,
    ) {
        self.provider = self
            .provider
            .take()
            .or_else(|| provider.map(String::from))
            .or_else(|| Some("anthropic".to_string()));
        self.model = self.model.take().or_else(|| model.map(String::from));
        self.permissions = self
            .permissions
            .or(permissions)
            .or(Some(PermissionLevel::ReadWrite));
        self.output_format = self
            .output_format
            .or(output_format)
            .or(Some(OutputFormat::Text));
    }
}

#[allow(
    clippy::print_stderr,
    reason = "Interactive approval prompts belong on stderr, not assistant output."
)]
#[expect(
    clippy::disallowed_methods,
    reason = "Interactive tool approval blocks on stdin and stderr by design."
)]
fn build_tool_approval(
    permissions: PermissionLevel,
    is_interactive: bool,
    styles: &'static Styles,
) -> ToolApprovalFn {
    let level = Arc::new(Mutex::new(permissions));

    Arc::new(move |tool_name: &str, _args: &serde_json::Value| {
        let current_level = *level.lock().expect("permission lock poisoned");

        if is_auto_approved(current_level, tool_category(tool_name)) {
            return Ok(());
        }

        if !is_interactive {
            return Err(format!(
                "{tool_name} tool denied at current permission level"
            ));
        }

        // Interactive prompt on stderr
        let category = tool_category(tool_name);
        eprint!(
            "Allow {} ({category})? [y]es / [n]o / [a]lways: ",
            styles.bold.apply_to(tool_name),
        );
        // `AgentToolCategory` derives strum::Display so it renders as the
        // canonical snake_case label (e.g. "read", "write").
        std::io::stderr().flush().ok();

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read input: {e}"))?;

        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(()),
            "a" | "always" => {
                let mut lvl = level.lock().expect("permission lock poisoned");
                *lvl = if category == AgentToolCategory::Write {
                    PermissionLevel::ReadWrite
                } else {
                    PermissionLevel::Full
                };
                Ok(())
            }
            _ => Err(format!("{tool_name} tool denied by user")),
        }
    })
}

fn summarizer_model_id(
    provider_id: &ProviderId,
    catalog: &Catalog,
    selected_model: &str,
) -> ModelHandle {
    let model = catalog
        .small_default_for([provider_id])
        .filter(|entry| entry.provider.id() == provider_id)
        .or_else(|| {
            catalog
                .enabled_provider(provider_id.as_str())?
                .default_offering()
        })
        .map_or_else(
            || selected_model.to_string(),
            |entry| entry.model.id().to_string(),
        );
    ModelHandle::new(provider_id.clone(), ModelId::new(model))
}

fn build_summarizer(
    provider_id: &ProviderId,
    model: &str,
    catalog: &Catalog,
    llm_client: Client,
) -> WebFetchSummarizer {
    WebFetchSummarizer {
        client:   llm_client,
        model_id: summarizer_model_id(provider_id, catalog, model),
    }
}

fn parse_provider(args: &AgentArgs) -> ProviderId {
    ProviderId::new(args.provider.as_deref().unwrap_or("anthropic"))
}

fn resolve_provider_id(
    catalog: &Catalog,
    args: &AgentArgs,
    eligible_providers: &std::collections::HashSet<ProviderId>,
) -> ProviderId {
    if args.provider.is_some() {
        let requested = parse_provider(args);
        return canonical_provider_id(catalog, &requested);
    }
    if let Some(model_id) = args.model.as_deref() {
        // A bare model selector picks the highest-priority eligible provider
        // offering it, matching how the client resolves the request.
        let matches = catalog.offerings_matching(model_id);
        if let Some(entry) = matches
            .iter()
            .find(|entry| eligible_providers.contains(entry.provider.id()))
            .or_else(|| matches.first())
        {
            return entry.provider.id().clone();
        }
    }
    let requested = parse_provider(args);
    canonical_provider_id(catalog, &requested)
}

/// The catalog id for `requested`, resolving aliases; the request itself when
/// the catalog does not know it, so the error names what the caller typed.
fn canonical_provider_id(catalog: &Catalog, requested: &ProviderId) -> ProviderId {
    catalog
        .enabled_provider(requested.as_str())
        .map_or_else(|| requested.clone(), |provider| provider.id().clone())
}

async fn standalone_llm_source() -> anyhow::Result<Arc<dyn CredentialProvider>> {
    let storage = Storage::new(default_storage_dir());
    let store = SecretStore::open(storage.sqlite_path(), storage.secrets_path())
        .await
        .context("opening the Fabro secret store")?;
    Ok(Arc::new(SqlVaultCredentialSource::new(Arc::new(store))))
}

fn profile_kind_for_provider(
    catalog: &Catalog,
    provider_id: &ProviderId,
    model: Option<&str>,
) -> anyhow::Result<AgentProfileKind> {
    catalog::agent_profile(catalog, provider_id.as_str(), model)
        .ok_or_else(|| anyhow::anyhow!("provider '{provider_id}' is not configured"))
}

fn ensure_provider_registered(client: &Client, provider_id: &ProviderId) -> anyhow::Result<()> {
    if client.available_providers().contains(provider_id) {
        return Ok(());
    }

    anyhow::bail!("LLM credentials not configured for provider '{provider_id}'");
}

fn format_tool_args(args: &serde_json::Value, cwd: &str) -> String {
    let cwd_prefix = if cwd.ends_with('/') {
        cwd.to_string()
    } else {
        format!("{cwd}/")
    };
    let Some(obj) = args.as_object() else {
        return args.to_string();
    };
    obj.iter()
        .map(|(k, v)| match v {
            serde_json::Value::String(s) => {
                let s = s.strip_prefix(&cwd_prefix).unwrap_or(s);
                let display = if s.len() > 80 {
                    format!("{}...", &s[..s.floor_char_boundary(77)])
                } else {
                    s.to_string()
                };
                format!("{k}={display:?}")
            }
            other => format!("{k}={other}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(
    clippy::print_stdout,
    reason = "Assistant responses are the CLI's primary stdout output."
)]
fn print_output(session: &Session, styles: &Styles) {
    for turn in session.history().turns() {
        if let Message::Assistant { content, .. } = turn {
            if !content.is_empty() {
                println!("{}", styles.render_markdown(content));
            }
        }
    }
}

#[allow(
    clippy::print_stderr,
    reason = "Session summaries are diagnostic metadata, not assistant output."
)]
fn print_summary(session: &Session, styles: &Styles) {
    let (mut turn_count, mut tool_call_count, mut total_tokens) = (0usize, 0usize, 0u64);
    for turn in session.history().turns() {
        if let Message::Assistant {
            tool_calls, usage, ..
        } = turn
        {
            turn_count += 1;
            tool_call_count += tool_calls.len();
            total_tokens = total_tokens.saturating_add(usage.total());
        }
    }
    let token_str = if total_tokens >= 1_000_000 {
        format!("{:.1}m", total_tokens as f64 / 1_000_000.0)
    } else if total_tokens >= 1000 {
        format!("{}k", total_tokens / 1000)
    } else {
        total_tokens.to_string()
    };
    eprintln!(
        "{}",
        styles.dim.apply_to(format!(
            "Done ({turn_count} turns, {tool_call_count} tools, {token_str} toks)"
        )),
    );
}

/// Middleware that logs LLM request/response summaries to stderr.
struct DebugMiddleware {
    styles: &'static Styles,
}

#[async_trait::async_trait]
impl Middleware for DebugMiddleware {
    #[allow(
        clippy::print_stderr,
        reason = "Debug middleware logs request and response summaries to stderr."
    )]
    async fn handle(&self, call: Call, next: Next) -> Result<Output, LlmError> {
        let s = self.styles;
        eprintln!(
            "{}",
            s.dim.apply_to(format!(
                "[debug] request: model={} messages={} tools={}",
                call.route().handle(),
                call.request().messages().len(),
                call.request().tools().len(),
            )),
        );
        let output = next.run(call).await?;
        if let Output::Complete(response) = &output {
            eprintln!(
                "{}",
                s.dim.apply_to(format!(
                    "[debug] response: model={} finish={:?} usage=({}/{}/{})",
                    response.model,
                    response.finish_reason,
                    response.usage.input,
                    response.usage.output,
                    response.usage.total(),
                )),
            );
        }
        Ok(output)
    }
}

/// Middleware that logs full LLM request/response JSON to stderr.
struct VerboseMiddleware {
    styles: &'static Styles,
}

#[async_trait::async_trait]
impl Middleware for VerboseMiddleware {
    #[allow(
        clippy::print_stderr,
        reason = "Verbose middleware dumps full request and response JSON to stderr."
    )]
    async fn handle(&self, call: Call, next: Next) -> Result<Output, LlmError> {
        let s = self.styles;
        eprintln!(
            "{}\n{}",
            s.dim.apply_to("[verbose] request:"),
            serde_json::to_string_pretty(call.request())
                .unwrap_or_else(|e| format!("<serialize error: {e}>"))
        );
        let output = next.run(call).await?;
        if let Output::Complete(response) = &output {
            eprintln!(
                "{}\n{}",
                s.dim.apply_to("[verbose] response:"),
                serde_json::to_string_pretty(response)
                    .unwrap_or_else(|e| format!("<serialize error: {e}>"))
            );
        }
        Ok(output)
    }
}

/// Client options for the standalone agent: standard retries plus the
/// requested diagnostic middleware.
fn cli_client_options(args: &AgentArgs, styles: &'static Styles) -> ClientOptions {
    let options = ClientOptions::standard();
    if args.verbose {
        options.with_middleware(Arc::new(VerboseMiddleware { styles }))
    } else if args.debug {
        options.with_middleware(Arc::new(DebugMiddleware { styles }))
    } else {
        options
    }
}

/// The catalog the standalone agent runs against: the lithos built-ins and
/// the operator's `[llm]` overlay from the active settings file.
#[expect(
    clippy::disallowed_methods,
    reason = "Standalone agent honors OPENAI_BASE_URL from the process environment."
)]
fn standalone_catalog() -> anyhow::Result<Arc<Catalog>> {
    let overlay =
        fabro_config::load_llm_overlay(None).context("failed to load the LLM settings overlay")?;
    let catalog = fabro_llm::build_catalog(&overlay, &|name| std::env::var(name).ok())
        .context("failed to build standalone agent LLM catalog")?;
    Ok(Arc::new(catalog))
}

pub async fn run_with_args(
    args: AgentArgs,
    mcp_servers: Vec<McpServerSettings>,
) -> anyhow::Result<()> {
    let llm_source = standalone_llm_source().await?;
    let catalog = standalone_catalog()?;
    run_with_args_and_source_and_catalog(args, llm_source, mcp_servers, catalog).await
}

#[allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "Assistant output stays on stdout while prompts and diagnostics use stderr."
)]
pub async fn run_with_args_and_source_and_catalog(
    args: AgentArgs,
    llm_source: Arc<dyn CredentialProvider>,
    mcp_servers: Vec<McpServerSettings>,
    catalog: Arc<Catalog>,
) -> anyhow::Result<()> {
    // Resolve color support once, leak to get 'static lifetime for use across
    // threads
    let styles: &'static Styles = Box::leak(Box::new(Styles::detect_stderr()));
    let built = fabro_llm::build_client(
        Catalog::clone(&catalog),
        llm_source,
        cli_client_options(&args, styles),
    )
    .await
    .context("Failed to create LLM client")?;
    for issue in &built.build_issues {
        eprintln!(
            "{}",
            styles.dim.apply_to(format!(
                "[llm] provider '{}' is unavailable: {}",
                issue.provider, issue.cause
            ))
        );
    }
    run_with_args_and_client_and_catalog_styled(args, built.client, mcp_servers, catalog, styles)
        .await
}

/// Run against an already-built client, such as the `fabro exec` gateway
/// client. Diagnostic middleware is the caller's responsibility.
#[allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "Assistant output stays on stdout while prompts and diagnostics use stderr."
)]
pub async fn run_with_args_and_client_and_catalog(
    args: AgentArgs,
    client: Client,
    mcp_servers: Vec<McpServerSettings>,
    catalog: Arc<Catalog>,
) -> anyhow::Result<()> {
    let styles: &'static Styles = Box::leak(Box::new(Styles::detect_stderr()));
    run_with_args_and_client_and_catalog_styled(args, client, mcp_servers, catalog, styles).await
}

/// Client options a caller building its own client can use so `--debug` and
/// `--verbose` behave the same as with the standalone client.
#[must_use]
pub fn diagnostic_client_options(args: &AgentArgs) -> ClientOptions {
    let styles: &'static Styles = Box::leak(Box::new(Styles::detect_stderr()));
    cli_client_options(args, styles)
}

#[allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "Assistant output stays on stdout while prompts and diagnostics use stderr."
)]
async fn run_with_args_and_client_and_catalog_styled(
    args: AgentArgs,
    client: Client,
    mcp_servers: Vec<McpServerSettings>,
    catalog: Arc<Catalog>,
    styles: &'static Styles,
) -> anyhow::Result<()> {
    let available: std::collections::HashSet<ProviderId> =
        client.available_providers().iter().cloned().collect();
    let provider_id = resolve_provider_id(&catalog, &args, &available);
    ensure_provider_registered(&client, &provider_id)?;

    let model = if let Some(model) = args.model.clone() {
        model
    } else {
        catalog
            .enabled_provider(provider_id.as_str())
            .and_then(CatalogProvider::default_offering)
            .map(|entry| entry.model.id().to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "provider '{provider_id}' has no default model in the catalog; pass --model explicitly"
                )
            })?
    };
    let profile_kind = profile_kind_for_provider(&catalog, &provider_id, Some(&model))?;
    eprintln!("{}", styles.dim.apply_to(format!("Using model: {model}")));
    let tool_secrets = cli_tool_secrets();
    let profile_builder = AgentProfileBuilder::new(
        profile_kind,
        provider_id.clone(),
        &model,
        Arc::clone(&catalog),
    );
    let profile_builder = if profile_kind.uses_codex_core_tools() {
        profile_builder
    } else {
        profile_builder.with_web_fetch_summarizer(Some(build_summarizer(
            &provider_id,
            &model,
            &catalog,
            client.clone(),
        )))
    };
    let profile_builder = profile_builder.with_tool_secrets(tool_secrets);
    let mut profile = profile_builder.build();

    // Build sandbox
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd_str = cwd.to_string_lossy().to_string();
    let env: Arc<dyn Sandbox> = Arc::new(LocalSandbox::new(cwd));

    // Build tool approval callback
    let permissions = args.permissions.unwrap_or(PermissionLevel::ReadWrite);
    #[expect(
        clippy::disallowed_methods,
        reason = "is_terminal() on stdin is a non-blocking fstat; no actual I/O performed"
    )]
    let is_interactive = std::io::stdin().is_terminal() && !args.auto_approve;
    let tool_approval = build_tool_approval(permissions, is_interactive, styles);
    let tool_hooks: Arc<dyn ToolHookCallback> = Arc::new(ToolApprovalAdapter(tool_approval));

    let config = SessionOptions {
        tool_hooks: Some(tool_hooks.clone()),
        permission_level: Some(permissions),
        skill_dirs: args.skills_dir.map(|d| vec![d]),
        mcp_servers,
        ..SessionOptions::default()
    };

    // Register subagent tools
    let supervisor = SubAgentSupervisor::new(config.max_subagent_depth);
    let supervisor_for_session = supervisor.clone();
    let factory_client = client.clone();
    let factory_profile_builder = profile_builder;
    let factory_env = Arc::clone(&env);
    let factory_hooks = config.tool_hooks.clone();
    let factory_permission_level = config.permission_level;
    let factory: SessionFactory = Arc::new(move || {
        let child_profile = factory_profile_builder.build();
        let child_profile: Arc<dyn AgentProfile> = Arc::from(child_profile);
        Session::new(
            factory_client.clone(),
            child_profile,
            Arc::clone(&factory_env),
            SessionOptions {
                tool_hooks: factory_hooks.clone(),
                permission_level: factory_permission_level,
                ..SessionOptions::default()
            },
            None,
        )
    });
    profile.register_subagent_tools(supervisor.clone(), factory, 0);
    let profile: Arc<dyn AgentProfile> = Arc::from(profile);

    let mut session = Session::new(client, profile, env, config, Some(supervisor_for_session));

    // Wire subagent event callback to parent session's emitter
    supervisor.set_event_callback(session.sub_agent_event_callback());

    // SIGINT handler
    let cancel_token = session.cancel_token();
    let interrupt_reason = session.interrupt_reason_handle();
    tokio::spawn(async move {
        signal::ctrl_c().await.ok();
        {
            let mut guard = interrupt_reason
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.is_none() {
                *guard = Some(InterruptReason::Cancelled);
            }
        }
        cancel_token.cancel();
    });

    // Subscribe to events
    let verbose = args.verbose;
    let output_format = args.output_format.unwrap_or(OutputFormat::Text);
    let mut rx = session.subscribe();
    tokio::spawn(async move {
        match output_format {
            OutputFormat::Json => {
                let mut stdout = stdout();
                while let Ok(event) = rx.recv().await {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let _ = stdout.write_all(json.as_bytes()).await;
                        let _ = stdout.write_all(b"\n").await;
                        let _ = stdout.flush().await;
                    }
                }
            }
            OutputFormat::Text => {
                let s = styles;
                while let Ok(event) = rx.recv().await {
                    let child_prefix = if event.parent_session_id.is_some() {
                        format!("[child {}] ", event.session_id)
                    } else {
                        String::new()
                    };
                    match &event.event {
                        AgentEvent::ToolCallStarted {
                            tool_name,
                            arguments,
                            ..
                        } => {
                            eprintln!(
                                "  {} {}{}",
                                s.dim.apply_to("\u{25cf}"),
                                s.bold_cyan.apply_to(format!("{child_prefix}{tool_name}")),
                                s.dim.apply_to(format!(
                                    "({})",
                                    format_tool_args(arguments, &cwd_str)
                                )),
                            );
                        }
                        AgentEvent::ToolCallCompleted {
                            tool_name,
                            output,
                            is_error,
                            ..
                        } if verbose => {
                            let label = if *is_error {
                                "tool error"
                            } else {
                                "tool result"
                            };
                            eprintln!(
                                "  {}\n{}",
                                s.dim
                                    .apply_to(format!("[{label}] {child_prefix}{tool_name}:")),
                                serde_json::to_string_pretty(output)
                                    .unwrap_or_else(|_| output.to_string()),
                            );
                        }
                        AgentEvent::Error { error } => {
                            eprintln!(
                                "  {}",
                                s.red.apply_to(format!("\u{2717} {child_prefix}{error}")),
                            );
                        }
                        AgentEvent::SubAgentSpawned {
                            agent_id,
                            depth,
                            task,
                            generation,
                        }
                        | AgentEvent::SubAgentTurnStarted {
                            agent_id,
                            depth,
                            task,
                            generation,
                        } => {
                            let started =
                                if matches!(event.event, AgentEvent::SubAgentSpawned { .. }) {
                                    "spawned"
                                } else {
                                    "turn started"
                                };
                            let task_preview = if task.len() > 60 {
                                &task[..task.floor_char_boundary(60)]
                            } else {
                                task
                            };
                            eprintln!(
                                "  {}",
                                s.dim.apply_to(format!(
                                    "{child_prefix}\u{25b6} subagent {agent_id} {started} (depth={depth}, generation={generation}) task={task_preview:?}"
                                )),
                            );
                        }
                        AgentEvent::SubAgentCompleted {
                            agent_id,
                            depth,
                            generation,
                            success,
                            turns_used,
                        } => {
                            eprintln!(
                                "  {}",
                                s.dim.apply_to(format!(
                                    "{child_prefix}\u{25a0} subagent {agent_id} completed (depth={depth}, generation={generation}, success={success}, turns={turns_used})"
                                )),
                            );
                        }
                        AgentEvent::SubAgentFailed {
                            agent_id,
                            depth,
                            generation,
                            error,
                        } => {
                            eprintln!(
                                "  {}",
                                s.red.apply_to(format!(
                                    "{child_prefix}\u{2717} subagent {agent_id} failed (depth={depth}, generation={generation}): {error}"
                                )),
                            );
                        }
                        AgentEvent::SubAgentClosed {
                            agent_id,
                            depth,
                            generation,
                        } => {
                            eprintln!(
                                "  {}",
                                s.dim.apply_to(format!(
                                    "{child_prefix}\u{25a0} subagent {agent_id} closed (depth={depth}, generation={generation})"
                                )),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    // Initialize and run
    let result = match session.initialize().await {
        Ok(()) => session.process_input(&args.prompt).await,
        Err(error) => Err(error),
    };
    let shutdown_reason = if result.is_ok() {
        SessionShutdownReason::Completed
    } else if session.cancel_token().is_cancelled() {
        SessionShutdownReason::Cancelled
    } else {
        SessionShutdownReason::Error
    };
    session.shutdown(shutdown_reason).await;

    if matches!(output_format, OutputFormat::Text) {
        // Print assistant text to stdout
        print_output(&session, styles);

        // Print completion summary to stderr
        print_summary(&session, styles);
    }

    // Propagate errors for exit code
    result?;
    Ok(())
}

pub async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut args = cli.args;
    args.apply_cli_defaults(None, None, None, None);
    run_with_args(args, Vec::new()).await
}

#[cfg(test)]
mod tests {
    use fabro_llm::test_support::{
        client_with_adapters, test_catalog as fabro_test_catalog, test_catalog_with_overlay,
    };
    use lithos_llm::catalog::builtin;
    use serde_json::json;

    use super::*;

    static NO_COLOR: std::sync::LazyLock<Styles> = std::sync::LazyLock::new(|| Styles::new(false));

    // tool_category tests

    #[test]
    fn tool_category_read_tools() {
        assert_eq!(tool_category("read_file"), AgentToolCategory::Read);
        assert_eq!(tool_category("read_many_files"), AgentToolCategory::Read);
        assert_eq!(tool_category("grep"), AgentToolCategory::Read);
        assert_eq!(tool_category("glob"), AgentToolCategory::Read);
        assert_eq!(tool_category("list_dir"), AgentToolCategory::Read);
    }

    #[test]
    fn tool_category_write_tools() {
        assert_eq!(tool_category("write_file"), AgentToolCategory::Write);
        assert_eq!(tool_category("edit_file"), AgentToolCategory::Write);
        assert_eq!(tool_category("apply_patch"), AgentToolCategory::Write);
    }

    #[test]
    fn tool_category_shell() {
        assert_eq!(tool_category("shell"), AgentToolCategory::Shell);
    }

    #[test]
    fn tool_category_subagent_tools() {
        assert_eq!(tool_category("spawn_agent"), AgentToolCategory::Subagent);
        assert_eq!(tool_category("send_input"), AgentToolCategory::Subagent);
        assert_eq!(tool_category("wait"), AgentToolCategory::Subagent);
        assert_eq!(tool_category("close_agent"), AgentToolCategory::Subagent);
    }

    #[test]
    fn tool_category_unknown_defaults_to_shell() {
        assert_eq!(tool_category("some_random_tool"), AgentToolCategory::Shell);
    }

    // is_auto_approved tests

    #[test]
    fn is_auto_approved_read_only() {
        assert!(is_auto_approved(
            PermissionLevel::ReadOnly,
            AgentToolCategory::Read
        ));
        assert!(is_auto_approved(
            PermissionLevel::ReadOnly,
            AgentToolCategory::Subagent
        ));
        assert!(!is_auto_approved(
            PermissionLevel::ReadOnly,
            AgentToolCategory::Write
        ));
        assert!(!is_auto_approved(
            PermissionLevel::ReadOnly,
            AgentToolCategory::Shell
        ));
    }

    #[test]
    fn is_auto_approved_read_write() {
        assert!(is_auto_approved(
            PermissionLevel::ReadWrite,
            AgentToolCategory::Read
        ));
        assert!(is_auto_approved(
            PermissionLevel::ReadWrite,
            AgentToolCategory::Subagent
        ));
        assert!(is_auto_approved(
            PermissionLevel::ReadWrite,
            AgentToolCategory::Write
        ));
        assert!(!is_auto_approved(
            PermissionLevel::ReadWrite,
            AgentToolCategory::Shell
        ));
    }

    #[test]
    fn is_auto_approved_full() {
        assert!(is_auto_approved(
            PermissionLevel::Full,
            AgentToolCategory::Read
        ));
        assert!(is_auto_approved(
            PermissionLevel::Full,
            AgentToolCategory::Subagent
        ));
        assert!(is_auto_approved(
            PermissionLevel::Full,
            AgentToolCategory::Write
        ));
        assert!(is_auto_approved(
            PermissionLevel::Full,
            AgentToolCategory::Shell
        ));
    }

    // build_tool_approval non-interactive tests

    #[test]
    fn build_tool_approval_read_only_allows_read() {
        let approval_fn = build_tool_approval(PermissionLevel::ReadOnly, false, &NO_COLOR);
        assert!(approval_fn("read_file", &json!({})).is_ok());
    }

    #[test]
    fn build_tool_approval_read_only_denies_write() {
        let approval_fn = build_tool_approval(PermissionLevel::ReadOnly, false, &NO_COLOR);
        let result = approval_fn("write_file", &json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("denied"));
    }

    #[test]
    fn build_tool_approval_read_write_denies_shell() {
        let approval_fn = build_tool_approval(PermissionLevel::ReadWrite, false, &NO_COLOR);
        let result = approval_fn("shell", &json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("denied"));
    }

    #[test]
    fn build_tool_approval_full_allows_shell() {
        let approval_fn = build_tool_approval(PermissionLevel::Full, false, &NO_COLOR);
        assert!(approval_fn("shell", &json!({})).is_ok());
    }

    fn enabled_ids(catalog: &Catalog) -> std::collections::HashSet<ProviderId> {
        catalog.enabled_provider_ids().into_iter().collect()
    }

    fn test_catalog() -> Arc<Catalog> {
        Arc::new(fabro_test_catalog())
    }

    /// An operator-defined OpenAI-compatible provider with one Claude model,
    /// the shape an `[llm]` overlay produces.
    const ACME_OVERLAY: &str = r#"
[providers.acme-aws]
display_name = "Acme AWS"
aliases = ["br"]
adapter = "openai-compatible"
codec = "openai-chat"
base_url = "https://example.invalid/v1"
auth = { type = "bearer" }
default_model = "acme-aws-claude"

[providers.acme-aws.metadata.agent]
profile = "openai"

[providers.acme-aws.models.acme-aws-claude]
display_name = "Acme AWS Claude"
api_model = "acme-aws-claude"
limits = { context_tokens = 1000, max_output_tokens = 500 }
capabilities = { text = true, tools = true }
family = "claude"

[providers.acme-aws.models.acme-aws-claude.metadata.agent]
profile = "anthropic"
"#;

    /// The same provider with no models, so its default comes from the
    /// operator's `--model` alone.
    const ACME_OVERLAY_WITHOUT_MODELS: &str = r#"
[providers.acme-aws]
display_name = "Acme AWS"
adapter = "openai-compatible"
codec = "openai-chat"
base_url = "https://example.invalid/v1"
auth = { type = "bearer" }
allow_passthrough = true

[providers.acme-aws.metadata.agent]
profile = "openai"
"#;

    fn acme_catalog() -> Catalog {
        test_catalog_with_overlay(ACME_OVERLAY)
    }

    fn args_with(provider: Option<&str>, model: Option<&str>) -> AgentArgs {
        AgentArgs {
            prompt:        "test".to_string(),
            provider:      provider.map(str::to_string),
            model:         model.map(str::to_string),
            permissions:   None,
            auto_approve:  false,
            debug:         false,
            verbose:       false,
            skills_dir:    None,
            output_format: None,
        }
    }

    #[test]
    fn ensure_provider_registered_reports_missing_credentials() {
        let client = client_with_adapters(Vec::new(), ClientOptions::default());
        let error = ensure_provider_registered(&client, &builtin::anthropic()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "LLM credentials not configured for provider 'anthropic'"
        );
    }

    #[test]
    fn profile_kind_accepts_custom_catalog_provider() {
        let catalog = acme_catalog();
        let args = args_with(Some("acme-aws"), None);

        let provider_id = parse_provider(&args);
        assert_eq!(provider_id, ProviderId::new("acme-aws"));
        assert_eq!(
            profile_kind_for_provider(&catalog, &provider_id, None).unwrap(),
            AgentProfileKind::OpenAi
        );
    }

    #[test]
    fn standalone_provider_resolution_uses_catalog_model_provider_when_provider_omitted() {
        let catalog = acme_catalog();
        let args = args_with(None, Some("acme-aws-claude"));

        assert_eq!(
            resolve_provider_id(&catalog, &args, &enabled_ids(&catalog)),
            ProviderId::new("acme-aws")
        );
    }

    #[test]
    fn standalone_provider_resolution_canonicalizes_explicit_provider_alias() {
        let catalog = acme_catalog();
        let args = args_with(Some("br"), None);

        assert_eq!(
            resolve_provider_id(&catalog, &args, &enabled_ids(&catalog)),
            ProviderId::new("acme-aws")
        );
    }

    #[test]
    fn standalone_profile_kind_uses_model_agent_profile_override() {
        let catalog = acme_catalog();

        assert_eq!(
            profile_kind_for_provider(
                &catalog,
                &ProviderId::new("acme-aws"),
                Some("acme-aws-claude")
            )
            .unwrap(),
            AgentProfileKind::Anthropic
        );
    }

    #[test]
    fn summarizer_model_id_uses_selected_model_for_custom_provider_without_default() {
        let catalog = test_catalog_with_overlay(ACME_OVERLAY_WITHOUT_MODELS);
        let provider_id = ProviderId::new("acme-aws");

        let model_id = summarizer_model_id(&provider_id, &catalog, "acme-aws-claude-sonnet-4-6");

        assert_eq!(model_id.provider(), &provider_id);
        assert_eq!(model_id.model().as_str(), "acme-aws-claude-sonnet-4-6");
    }

    #[test]
    fn summarizer_model_id_prefers_the_provider_small_default() {
        let catalog = test_catalog();
        let model_id = summarizer_model_id(&builtin::openai(), &catalog, "gpt-5.4");

        assert_eq!(model_id.provider(), &builtin::openai());
        assert_eq!(model_id.model().as_str(), "gpt-5.4-mini");
    }

    // subagent tool registration tests

    #[test]
    fn build_profile_can_register_subagent_tools() {
        let mut profile = AgentProfileBuilder::new(
            AgentProfileKind::Anthropic,
            builtin::anthropic(),
            "model",
            test_catalog(),
        )
        .build();
        let supervisor = SubAgentSupervisor::new(1);
        let factory: SessionFactory = Arc::new(|| {
            panic!("factory should not be called in this test");
        });
        profile.register_subagent_tools(supervisor, factory, 0);

        let names = profile.tool_registry().names();
        assert!(names.contains(&"spawn_agent".to_string()));
        assert!(names.contains(&"send_input".to_string()));
        assert!(names.contains(&"wait".to_string()));
        assert!(names.contains(&"close_agent".to_string()));
    }
}
