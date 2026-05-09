use anyhow::Result;
use clap::{Args, Subcommand};

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_empty, print_value, Column, OutputFormat};

#[derive(Debug, Args)]
pub struct ReleasesArgs {
    #[command(subcommand)]
    pub cmd: ReleasesCmd,
}

#[derive(Debug, Subcommand)]
pub enum ReleasesCmd {
    /// List releases.
    List(ListArgs),
    /// Fetch a single release by version.
    Get { version: String },
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, short = 'n', default_value_t = 25)]
    pub limit: usize,
    #[arg(long, default_value_t = 100)]
    pub max: usize,
}

pub async fn run(
    args: ReleasesArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        ReleasesCmd::List(a) => list(a, client, auth, sentry_project, format).await,
        ReleasesCmd::Get { version } => get(&version, client, auth, format).await,
    }
}

async fn list(
    args: ListArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let mut query: Vec<(&str, String)> = vec![("per_page", args.limit.to_string())];
    if let Some(q) = args.query {
        query.push(("query", q));
    }
    if let Some(p) = sentry_project {
        query.push(("project", p.to_string()));
    }
    let path = format!("/organizations/{}/releases/", org);
    let items = client.paginate(&path, &query, args.max).await?;
    if items.is_empty() {
        return print_empty(format);
    }
    let value = serde_json::Value::Array(items);
    let cols = [
        Column::new("VERSION", &["version"]),
        Column::new("NEW_GROUPS", &["newGroups"]),
        Column::new("DATE_CREATED", &["dateCreated"]),
        Column::new("URL", &["url"]),
    ];
    print_value(format, &value, &cols)
}

async fn get(
    version: &str,
    client: &ApiClient,
    auth: &Resolved,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let encoded = urlencoding(version);
    let path = format!("/organizations/{}/releases/{}/", org, encoded);
    let value: serde_json::Value = client.get_json(&path, &[]).await?;
    print_value(format, &value, &[])
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
