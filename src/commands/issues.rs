use anyhow::Result;
use clap::{Args, Subcommand};
use serde_json::json;

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_empty, print_value, Column, OutputFormat};
use crate::time;

#[derive(Debug, Args)]
pub struct IssuesArgs {
    #[command(subcommand)]
    pub cmd: IssuesCmd,
}

#[derive(Debug, Subcommand)]
pub enum IssuesCmd {
    /// List issues matching a query.
    List(ListArgs),
    /// Fetch a single issue by ID or short ID.
    Get { id: String },
    /// List events for an issue.
    Events(EventsArgs),
    /// Update an issue (resolve / ignore / assign). Requires --yes.
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(default_value = "is:unresolved")]
    pub query: String,
    #[arg(long, short = 'f', default_value = "now-24h")]
    pub from: String,
    #[arg(long, short = 't', default_value = "now")]
    pub to: String,
    #[arg(long)]
    pub environment: Option<String>,
    #[arg(long, default_value = "date")]
    pub sort: String,
    #[arg(long, short = 'n', default_value_t = 25)]
    pub limit: usize,
    #[arg(long, default_value_t = 100)]
    pub max: usize,
    /// Show the original TITLE column instead of WHERE (function/file).
    #[arg(long)]
    pub full: bool,
}

#[derive(Debug, Args)]
pub struct EventsArgs {
    pub issue: String,
    #[arg(long)]
    pub latest: bool,
    #[arg(long, short = 'n', default_value_t = 25)]
    pub limit: usize,
    #[arg(long, default_value_t = 100)]
    pub max: usize,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub issue: String,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub assign_to: Option<String>,
    #[arg(long)]
    pub unassign: bool,
    /// Required to actually mutate state.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(
    args: IssuesArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        IssuesCmd::List(a) => list(a, client, auth, sentry_project, format).await,
        IssuesCmd::Get { id } => get(&id, client, auth, format).await,
        IssuesCmd::Events(a) => events(a, client, auth, format).await,
        IssuesCmd::Update(a) => update(a, client, auth, format).await,
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
    let from = time::to_iso(time::parse(&args.from)?);
    let to = time::to_iso(time::parse(&args.to)?);

    let mut query: Vec<(&str, String)> = vec![
        ("query", args.query.clone()),
        ("start", from),
        ("end", to),
        ("sort", args.sort.clone()),
        ("limit", args.limit.to_string()),
    ];
    if let Some(env) = &args.environment {
        query.push(("environment", env.clone()));
    }
    if let Some(proj) = sentry_project {
        query.push(("project", proj.to_string()));
    }

    let path = format!("/organizations/{}/issues/", org);
    let mut items = client.paginate(&path, &query, args.max).await?;
    if items.is_empty() {
        return print_empty(format);
    }
    if !args.full {
        for it in items.iter_mut() {
            if let Some(obj) = it.as_object_mut() {
                let metadata = obj.get("metadata");
                let function = metadata
                    .and_then(|m| m.get("function"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let filename = metadata
                    .and_then(|m| m.get("filename"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let culprit = obj.get("culprit").and_then(|v| v.as_str()).unwrap_or("");
                let where_str = match (function.is_empty(), filename.is_empty()) {
                    (false, false) => format!("{} ({})", function, filename),
                    (false, true) => function.to_string(),
                    (true, false) => filename.to_string(),
                    (true, true) => culprit.to_string(),
                };
                obj.insert("_where".into(), serde_json::Value::String(where_str));
            }
        }
    }
    let value = serde_json::Value::Array(items);
    let cols: &[Column] = if args.full {
        &[
            Column::new("SHORT_ID", &["shortId"]),
            Column::new("LEVEL", &["level"]),
            Column::new("COUNT", &["count"]),
            Column::new("USERS", &["userCount"]),
            Column::new("TITLE", &["title"]),
        ]
    } else {
        &[
            Column::new("SHORT_ID", &["shortId"]),
            Column::new("LEVEL", &["level"]),
            Column::new("COUNT", &["count"]),
            Column::new("USERS", &["userCount"]),
            Column::new("WHERE", &["_where"]),
        ]
    };
    print_value(format, &value, cols)
}

async fn get(id: &str, client: &ApiClient, auth: &Resolved, format: OutputFormat) -> Result<()> {
    let org = require_org(auth)?;
    let path = format!("/organizations/{}/issues/{}/", org, id);
    let value: serde_json::Value = client.get_json(&path, &[]).await?;
    print_value(format, &value, &[])
}

async fn events(
    args: EventsArgs,
    client: &ApiClient,
    auth: &Resolved,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    if args.latest {
        let path = format!("/organizations/{}/issues/{}/events/latest/", org, args.issue);
        let value: serde_json::Value = client.get_json(&path, &[]).await?;
        return print_value(format, &value, &[]);
    }
    let path = format!("/organizations/{}/issues/{}/events/", org, args.issue);
    let query: Vec<(&str, String)> = vec![("limit", args.limit.to_string())];
    let items = client.paginate(&path, &query, args.max).await?;
    if items.is_empty() {
        return print_empty(format);
    }
    let value = serde_json::Value::Array(items);
    let cols = [
        Column::new("EVENT_ID", &["eventID"]),
        Column::new("TIMESTAMP", &["dateCreated"]),
        Column::new("USER_ID", &["user", "id"]),
        Column::new("RELEASE", &["release", "version"]),
        Column::new("MESSAGE", &["message"]),
    ];
    print_value(format, &value, &cols)
}

async fn update(
    args: UpdateArgs,
    client: &ApiClient,
    auth: &Resolved,
    format: OutputFormat,
) -> Result<()> {
    if !args.yes {
        anyhow::bail!(
            "Refusing to mutate without --yes. Re-run with --yes to confirm the change."
        );
    }
    let org = require_org(auth)?;

    let mut body = serde_json::Map::new();
    if let Some(s) = args.status.as_deref() {
        body.insert("status".into(), json!(s));
    }
    if args.unassign {
        body.insert("assignedTo".into(), serde_json::Value::Null);
    } else if let Some(a) = args.assign_to.as_deref() {
        body.insert("assignedTo".into(), json!(a));
    }
    if body.is_empty() {
        anyhow::bail!("Nothing to update. Pass --status / --assign-to / --unassign.");
    }

    let url = client.url(&format!("/organizations/{}/issues/{}/", org, args.issue));
    let resp = client
        .send(
            client
                .request(reqwest::Method::PUT, &url)
                .json(&serde_json::Value::Object(body)),
        )
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Sentry API error ({}): {}", status, text);
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    print_value(format, &value, &[])
}
