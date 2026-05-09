use anyhow::Result;
use clap::{Args, Subcommand};

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_value, Column, OutputFormat};

#[derive(Debug, Args)]
pub struct ProjectsArgs {
    #[command(subcommand)]
    pub cmd: ProjectsCmd,
}

#[derive(Debug, Subcommand)]
pub enum ProjectsCmd {
    /// List Sentry projects in the current org.
    List,
    /// Get a single project by slug.
    Get {
        /// Sentry project slug.
        slug: String,
    },
}

pub async fn run(
    args: ProjectsArgs,
    client: &ApiClient,
    auth: &Resolved,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        ProjectsCmd::List => list(client, auth, format).await,
        ProjectsCmd::Get { slug } => get(client, auth, &slug, format).await,
    }
}

async fn list(client: &ApiClient, auth: &Resolved, format: OutputFormat) -> Result<()> {
    let org = require_org(auth)?;
    let path = format!("/organizations/{}/projects/", org);
    let items = client.paginate(&path, &[], 0).await?;
    if items.is_empty() {
        return crate::output::print_empty(format);
    }
    let value = serde_json::Value::Array(items);
    let cols = [
        Column::new("SLUG", &["slug"]),
        Column::new("PLATFORM", &["platform"]),
        Column::new("ID", &["id"]),
        Column::new("LAST_EVENT", &["lastEvent"]),
    ];
    print_value(format, &value, &cols)
}

async fn get(
    client: &ApiClient,
    auth: &Resolved,
    slug: &str,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let path = format!("/projects/{}/{}/", org, slug);
    let value: serde_json::Value = client.get_json(&path, &[]).await?;
    print_value(format, &value, &[])
}
