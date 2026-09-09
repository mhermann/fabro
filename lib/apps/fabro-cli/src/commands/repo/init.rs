#![expect(
    clippy::disallowed_methods,
    reason = "sync CLI `repo init` command: interactive prompts read from std::io::stdin"
)]

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tokio::process::Command as TokioCommand;
use tokio::task::spawn_blocking;

use crate::args::{RepoInitArgs, ServerTargetArgs};
use crate::command_context::CommandContext;

#[expect(
    clippy::disallowed_methods,
    reason = "This is a shared synchronous git helper used by repo deinit; async callers should use spawn_blocking."
)]
pub(super) fn git_repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!("not a git repository — run `git init` first");
    }
    Ok(PathBuf::from(
        String::from_utf8(output.stdout)
            .context("git output was not valid UTF-8")?
            .trim(),
    ))
}

pub(crate) async fn run_init(
    args: &RepoInitArgs,
    base_ctx: &CommandContext,
) -> Result<Vec<String>> {
    let printer = base_ctx.printer();
    let repo_root = spawn_blocking(git_repo_root)
        .await
        .context("git repo root task panicked")??;
    let mut created = Vec::new();

    let fabro_dir = repo_root.join(".fabro");
    let project_toml = fabro_dir.join("project.toml");
    if project_toml.exists() {
        bail!(
            "already initialized — .fabro/project.toml exists at {}",
            project_toml.display()
        );
    }

    std::fs::create_dir_all(&fabro_dir)
        .with_context(|| format!("failed to create {}", fabro_dir.display()))?;

    // Create .fabro/project.toml
    std::fs::write(
        &project_toml,
        "\
# Fabro project configuration
# https://docs.fabro.computer/getting-started/quick-start

_version = 1
",
    )
    .with_context(|| format!("failed to write {}", project_toml.display()))?;
    created.push(".fabro/project.toml".to_string());

    let green = console::Style::new().green();
    let bold = console::Style::new().bold();
    let dim = console::Style::new().dim();
    if !base_ctx.json_output() {
        fabro_util::printerr!(
            printer,
            "  {} {}",
            green.apply_to("✔"),
            dim.apply_to(".fabro/project.toml")
        );
    }

    // Create hello workflow directory
    let workflow_dir = repo_root.join(".fabro/workflows/hello");
    std::fs::create_dir_all(&workflow_dir)
        .with_context(|| format!("failed to create {}", workflow_dir.display()))?;

    // Create workflow.fabro
    let dot_path = workflow_dir.join("workflow.fabro");
    std::fs::write(
        &dot_path,
        r#"digraph Hello {
    graph [goal="Say hello and demonstrate a basic Fabro workflow"]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    greet [label="Greet", prompt="Say hello! Introduce yourself and explain that this is a test of the Fabro workflow engine."]

    start -> greet -> exit
}
"#,
    )
    .with_context(|| format!("failed to write {}", dot_path.display()))?;
    created.push(".fabro/workflows/hello/workflow.fabro".to_string());
    if !base_ctx.json_output() {
        fabro_util::printerr!(
            printer,
            "  {} {}",
            green.apply_to("✔"),
            dim.apply_to(".fabro/workflows/hello/workflow.fabro")
        );
    }

    // Leave the environment unspecified so the target server can supply its
    // default.
    let toml_path = workflow_dir.join("workflow.toml");
    std::fs::write(
        &toml_path,
        "\
_version = 1

[workflow]
graph = \"workflow.fabro\"

# Auto-create pull requests on successful workflow runs.
[run.pull_request]
enabled = true
draft = true
# auto_merge = true
",
    )
    .with_context(|| format!("failed to write {}", toml_path.display()))?;
    created.push(".fabro/workflows/hello/workflow.toml".to_string());
    if !base_ctx.json_output() {
        fabro_util::printerr!(
            printer,
            "  {} {}",
            green.apply_to("✔"),
            dim.apply_to(".fabro/workflows/hello/workflow.toml")
        );
    }

    if !base_ctx.json_output() {
        fabro_util::printerr!(
            printer,
            "\n{} Run a workflow with:\n\n  {}",
            bold.apply_to("Project initialized!"),
            console::Style::new()
                .cyan()
                .bold()
                .apply_to("fabro run hello")
        );
    }

    if !base_ctx.json_output() {
        check_github_app_installation(&args.target, base_ctx).await;
    }

    Ok(created)
}

/// Which server repo-check endpoint the detected remote should probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provider {
    Github,
    Forgejo,
}

async fn check_github_app_installation(target: &ServerTargetArgs, base_ctx: &CommandContext) {
    let printer = base_ctx.printer();
    // Get the git remote origin URL
    let output = match TokioCommand::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => {
            let yellow = console::Style::new().yellow();
            let dim = console::Style::new().dim();
            fabro_util::printerr!(
                printer,
                "\n  {} No git remote found — skipping GitHub check",
                yellow.apply_to("!")
            );
            fabro_util::printerr!(
                printer,
                "  {}",
                dim.apply_to(
                    "Run `git remote add origin <url>`, then `gh auth login` or `fabro install` to configure GitHub access"
                )
            );
            return;
        }
    };

    let remote_url = match String::from_utf8(output.stdout) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };

    // Classify the remote: a Forgejo instance remote probes the forgejo
    // check endpoint; a github.com remote keeps the GitHub App flow; any
    // other remote is skipped silently.
    let forgejo_base = fabro_forgejo::forgejo_base_url(None);
    let is_forgejo = forgejo_base
        .as_deref()
        .is_some_and(|base| fabro_types::is_forgejo_origin(&remote_url, base));
    let https_url = fabro_github::ssh_url_to_https(&remote_url);
    let provider_repo = if is_forgejo {
        fabro_forgejo::parse_forgejo_owner_repo(
            &fabro_forgejo::normalize_repo_origin_url(&https_url),
            forgejo_base.as_deref().unwrap_or_default(),
        )
        .map(|(owner, repo)| (Provider::Forgejo, owner, repo))
    } else {
        fabro_github::parse_github_owner_repo(&https_url)
            .map(|(owner, repo)| (Provider::Github, owner, repo))
    };
    // Not a supported remote — skip silently.
    let Ok((provider, owner, repo)) = provider_repo else {
        return;
    };

    let ctx = match base_ctx.with_target(target) {
        Ok(ctx) => ctx,
        Err(err) => {
            fabro_util::printerr!(
                printer,
                "\n  Warning: could not resolve fabro server settings: {err}"
            );
            return;
        }
    };

    let server = match ctx.server().await {
        Ok(server) => server,
        Err(err) => {
            fabro_util::printerr!(
                printer,
                "\n  Warning: could not connect to fabro server: {err}"
            );
            return;
        }
    };

    let check = match match provider {
        Provider::Forgejo => server.get_forgejo_repo(&owner, &repo).await,
        Provider::Github => server.get_github_repo(&owner, &repo).await,
    } {
        Ok(response) => response,
        Err(err) => {
            fabro_util::printerr!(printer, "\n  Warning: could not check GitHub access: {err}");
            return;
        }
    };

    let provider_label = match provider {
        Provider::Github => "GitHub",
        Provider::Forgejo => "Forgejo",
    };
    if check.accessible {
        let green = console::Style::new().green();
        fabro_util::printerr!(
            printer,
            "\n  {} {provider_label} access is configured for {owner}/{repo}",
            green.apply_to("✔")
        );
        return;
    }

    let yellow = console::Style::new().yellow();
    fabro_util::printerr!(
        printer,
        "\n  {} {provider_label} access is not available for {owner}/{repo}",
        yellow.apply_to("!")
    );
    if let Some(url) = &check.install_url {
        fabro_util::printerr!(printer, "  Install at: {url}");
    } else if provider == Provider::Forgejo {
        fabro_util::printerr!(
            printer,
            "  Run `fabro install forgejo` (or set FORGEJO_TOKEN), then try again."
        );
    } else {
        fabro_util::printerr!(
            printer,
            "  Run `gh auth login` or `fabro install`, then try again."
        );
    }

    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        fabro_util::printerr!(printer, "  Press Enter to continue after installing...");
        let _ = spawn_blocking(|| {
            let mut buf = String::new();
            let _ = std::io::stdin().read_line(&mut buf);
        })
        .await;

        match match provider {
            Provider::Forgejo => server.get_forgejo_repo(&owner, &repo).await,
            Provider::Github => server.get_github_repo(&owner, &repo).await,
        } {
            Ok(response) => {
                if response.accessible {
                    let green = console::Style::new().green();
                    fabro_util::printerr!(
                        printer,
                        "  {} {provider_label} access is configured for {owner}/{repo}",
                        green.apply_to("✔")
                    );
                } else {
                    fabro_util::printerr!(
                        printer,
                        "  {provider_label} access is still unavailable."
                    );
                    if let Some(url) = &check.install_url {
                        fabro_util::printerr!(printer, "  Install at: {url}");
                    }
                }
            }
            Err(err) => {
                fabro_util::printerr!(
                    printer,
                    "  Warning: could not re-check {provider_label} access: {err}"
                );
            }
        }
    }
}
