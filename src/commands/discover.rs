use anyhow::Result;
use clap::{Args, Subcommand};

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_empty, print_value, OutputFormat};
use crate::time;

#[derive(Debug, Args)]
pub struct DiscoverArgs {
    #[command(subcommand)]
    pub cmd: DiscoverCmd,
}

#[derive(Debug, Subcommand)]
pub enum DiscoverCmd {
    /// Run a Discover events query.
    Query(QueryArgs),
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    #[arg(default_value = "")]
    pub query: String,
    #[arg(long, default_values_t = ["id".to_string(), "title".to_string(), "timestamp".to_string()])]
    pub field: Vec<String>,
    #[arg(long, short = 'f', default_value = "now-24h")]
    pub from: String,
    #[arg(long, short = 't', default_value = "now")]
    pub to: String,
    #[arg(long, default_value = "-timestamp")]
    pub sort: String,
    #[arg(long, short = 'n', default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value_t = 1000)]
    pub max: usize,
    #[arg(long)]
    pub environment: Option<String>,
    #[arg(long, default_value = "errors")]
    pub dataset: String,
}

pub async fn run(
    args: DiscoverArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        DiscoverCmd::Query(a) => query(a, client, auth, sentry_project, format).await,
    }
}

async fn query(
    args: QueryArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let from = time::to_iso(time::parse(&args.from)?);
    let to = time::to_iso(time::parse(&args.to)?);

    let mut query: Vec<(&str, String)> = vec![
        ("query", args.query.clone()),
        ("start", from),
        ("end", to),
        ("sort", args.sort.clone()),
        ("per_page", args.limit.to_string()),
        ("dataset", args.dataset.clone()),
    ];
    for f in &args.field {
        query.push(("field", f.clone()));
    }
    if let Some(env) = &args.environment {
        query.push(("environment", env.clone()));
    }
    if let Some(proj) = sentry_project {
        query.push(("project", proj.to_string()));
    }

    let path = format!("/organizations/{}/events/", org);
    let url = client.url(&path);
    let resp = client.send(client.request(reqwest::Method::GET, &url).query(&query)).await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Sentry API error ({}): {}", status, text);
    }
    let value: serde_json::Value = serde_json::from_str(&text)?;

    // Discover responses wrap rows under `data`.
    let rows = value
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let arr = match rows {
        serde_json::Value::Array(a) => a,
        _ => vec![],
    };
    if arr.is_empty() {
        return print_empty(format);
    }
    let _ = args.max; // pagination via Link not yet wired through for this endpoint
    print_value(format, &serde_json::Value::Array(arr), &[])
}
