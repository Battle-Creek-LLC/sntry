use anyhow::Result;
use clap::{Args, Subcommand};

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_value, OutputFormat};

#[derive(Debug, Args)]
pub struct EventsArgs {
    #[command(subcommand)]
    pub cmd: EventsCmd,
}

#[derive(Debug, Subcommand)]
pub enum EventsCmd {
    /// Fetch a full event by ID.
    Get {
        event_id: String,
    },
}

pub async fn run(
    args: EventsArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        EventsCmd::Get { event_id } => get(&event_id, client, auth, sentry_project, format).await,
    }
}

async fn get(
    event_id: &str,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    if let Some(proj) = sentry_project {
        let path = format!("/projects/{}/{}/events/{}/", org, proj, event_id);
        let value: serde_json::Value = client.get_json(&path, &[]).await?;
        return print_value(format, &value, &[]);
    }
    // Without a project slug, fall back to the org-wide event lookup.
    let path = format!("/organizations/{}/events/{}/", org, event_id);
    let value: serde_json::Value = client.get_json(&path, &[]).await?;
    print_value(format, &value, &[])
}
