use anyhow::Result;
use fabro_types::PullRequestLink;
use tracing::info;

use crate::args::PrLinkArgs;
use crate::command_context::CommandContext;
use crate::shared::{forgejo as shared_forgejo, print_json_pretty};

pub(super) async fn link_command(args: PrLinkArgs, base_ctx: &CommandContext) -> Result<()> {
    let forgejo_link = forgejo_link_for_url(base_ctx, &args.url).await;
    let (ctx, client, run_id) =
        super::resolve_run_selector(base_ctx, &args.server, &args.run_id).await?;
    let record = client.link_run_pull_request(&run_id, args.url).await?;

    info!(
        pr_url = %record.html_url(),
        number = record.number,
        "Linked pull request"
    );

    if ctx.json_output() {
        print_json_pretty(&record)?;
    } else {
        fabro_util::printout!(
            ctx.printer(),
            "Linked pull request: {} ({})",
            record.html_url(),
            record_label(&record, forgejo_link.as_ref())
        );
    }

    Ok(())
}

/// Match a pull request URL against the configured Forgejo instance. The
/// server parses GitHub pull request URLs itself, so only URLs that fail the
/// GitHub parse are classified here; `None` covers GitHub URLs, unconfigured
/// instances, and URLs that match no configured instance.
async fn forgejo_link_for_url(base_ctx: &CommandContext, url: &str) -> Option<PullRequestLink> {
    if PullRequestLink::from_github_url(url).is_ok() {
        return None;
    }
    let config = shared_forgejo::resolve_forgejo_config(base_ctx.storage_dir())
        .await
        .ok()??;
    PullRequestLink::from_forgejo_url(config.instance.as_str(), url).ok()
}

fn record_label(record: &PullRequestLink, forgejo_link: Option<&PullRequestLink>) -> String {
    let link = forgejo_link.unwrap_or(record);
    if link.forge.is_some() {
        format!("forgejo #{}", link.number)
    } else {
        format!("github #{}", link.number)
    }
}
