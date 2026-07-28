use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use serde_json::{json, Value};

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_empty, print_value, Column, OutputFormat};
use crate::time;

#[derive(Debug, Args)]
pub struct MonitorsArgs {
    #[command(subcommand)]
    pub cmd: MonitorsCmd,
}

#[derive(Debug, Subcommand)]
pub enum MonitorsCmd {
    /// List cron monitors for the org.
    List(ListArgs),
    /// Fetch a single monitor by slug.
    Get { slug: String },
    /// List recent check-ins for a monitor.
    Checkins(CheckinsArgs),
    /// Flag misconfigured or unhealthy monitors.
    Audit(AuditArgs),
    /// Create a monitor. Prints the payload unless --yes is passed.
    Create(CreateArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Filter: active | disabled (monitor) or ok | error (any environment).
    #[arg(long)]
    pub status: Option<String>,
    /// Filter by monitor environment.
    #[arg(long)]
    pub env: Option<String>,
    #[arg(long, short = 'n', default_value_t = 25)]
    pub limit: usize,
    #[arg(long, default_value_t = 100)]
    pub max: usize,
}

#[derive(Debug, Args)]
pub struct CheckinsArgs {
    pub slug: String,
    /// Filter by environment.
    #[arg(long)]
    pub env: Option<String>,
    #[arg(long, short = 'n', default_value_t = 25)]
    pub limit: usize,
    #[arg(long, default_value_t = 100)]
    pub max: usize,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// Flag monitors with no check-in within this window, e.g. 12h, 7d.
    #[arg(long, default_value = "7d")]
    pub stale: String,
    /// Exit 2 if anything is flagged (for CI).
    #[arg(long)]
    pub fail_on_findings: bool,
    #[arg(long, default_value_t = 500)]
    pub max: usize,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Monitor slug (unique within the org). Optional with --from-file.
    pub slug: Option<String>,
    /// Display name. Defaults to the slug.
    #[arg(long)]
    pub name: Option<String>,
    /// Crontab schedule, e.g. "0 * * * *".
    #[arg(long, conflicts_with = "interval")]
    pub crontab: Option<String>,
    /// Interval schedule: count and unit, e.g. --interval 30 minute.
    #[arg(long, num_args = 2, value_names = ["N", "UNIT"])]
    pub interval: Option<Vec<String>>,
    /// IANA timezone the schedule runs in.
    #[arg(long, default_value = "UTC")]
    pub timezone: String,
    /// Minutes late before a check-in counts as missed.
    #[arg(long)]
    pub checkin_margin: Option<u64>,
    /// Minutes before an in-progress check-in is marked failed.
    #[arg(long)]
    pub max_runtime: Option<u64>,
    /// Failed check-ins before an issue is created.
    #[arg(long)]
    pub failure_threshold: Option<u64>,
    /// OK check-ins before the issue resolves.
    #[arg(long)]
    pub recovery_threshold: Option<u64>,
    /// Owner actor, e.g. team:6 or user:51.
    #[arg(long)]
    pub owner: Option<String>,
    /// Read the full monitor JSON from a file ("-" = stdin) instead of flags.
    #[arg(long)]
    pub from_file: Option<String>,
    /// Required to actually create. Without it the payload is printed as a dry run.
    #[arg(long)]
    pub yes: bool,
}

pub async fn run(
    args: MonitorsArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        MonitorsCmd::List(a) => list(a, client, auth, sentry_project, format).await,
        MonitorsCmd::Get { slug } => get(&slug, client, auth, format).await,
        MonitorsCmd::Checkins(a) => checkins(a, client, auth, format).await,
        MonitorsCmd::Audit(a) => audit(a, client, auth, sentry_project, format).await,
        MonitorsCmd::Create(a) => create(a, client, auth, sentry_project, format).await,
    }
}

fn env_timestamps(monitor: &Value, key: &str) -> Option<DateTime<Utc>> {
    monitor
        .get("environments")?
        .as_array()?
        .iter()
        .filter_map(|e| e.get(key)?.as_str())
        .filter_map(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .max()
}

/// Adds `_lastCheckIn` / `_nextCheckIn` (latest across environments) so the
/// table columns have flat paths to point at.
fn annotate(monitor: &mut Value) {
    let last = env_timestamps(monitor, "lastCheckIn").map(time::to_iso);
    let next = env_timestamps(monitor, "nextCheckIn").map(time::to_iso);
    if let Some(obj) = monitor.as_object_mut() {
        obj.insert("_lastCheckIn".into(), last.map(Value::String).unwrap_or(Value::Null));
        obj.insert("_nextCheckIn".into(), next.map(Value::String).unwrap_or(Value::Null));
    }
}

fn env_statuses(monitor: &Value) -> Vec<(String, String)> {
    monitor
        .get("environments")
        .and_then(|e| e.as_array())
        .map(|envs| {
            envs.iter()
                .map(|e| {
                    (
                        e.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        e.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn fetch_monitors(
    client: &ApiClient,
    org: &str,
    sentry_project: Option<&str>,
    env: Option<&str>,
    limit: usize,
    max: usize,
) -> Result<Vec<Value>> {
    let mut query: Vec<(&str, String)> = vec![("per_page", limit.to_string())];
    if let Some(p) = sentry_project {
        query.push(("project", p.to_string()));
    }
    if let Some(e) = env {
        query.push(("environment", e.to_string()));
    }
    let path = format!("/organizations/{}/monitors/", org);
    client.paginate(&path, &query, max).await
}

async fn list(
    args: ListArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let mut items = fetch_monitors(
        client,
        org,
        sentry_project,
        args.env.as_deref(),
        args.limit,
        args.max,
    )
    .await?;

    if let Some(want) = args.status.as_deref() {
        items.retain(|m| {
            let top = m.get("status").and_then(|v| v.as_str()).unwrap_or("");
            top == want || env_statuses(m).iter().any(|(_, s)| s == want)
        });
    }
    if items.is_empty() {
        return print_empty(format);
    }
    for m in items.iter_mut() {
        annotate(m);
    }
    let value = Value::Array(items);
    let cols = [
        Column::new("SLUG", &["slug"]),
        Column::new("NAME", &["name"]),
        Column::new("SCHEDULE", &["config", "schedule"]),
        Column::new("STATUS", &["status"]),
        Column::new("MUTED", &["isMuted"]),
        Column::new("LAST-CHECKIN", &["_lastCheckIn"]),
        Column::new("NEXT-EXPECTED", &["_nextCheckIn"]),
    ];
    print_value(format, &value, &cols)
}

async fn get(slug: &str, client: &ApiClient, auth: &Resolved, format: OutputFormat) -> Result<()> {
    let org = require_org(auth)?;
    let path = format!("/organizations/{}/monitors/{}/", org, slug);
    let value: Value = client.get_json(&path, &[]).await?;
    print_value(format, &value, &[])
}

async fn checkins(
    args: CheckinsArgs,
    client: &ApiClient,
    auth: &Resolved,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let mut query: Vec<(&str, String)> = vec![("per_page", args.limit.to_string())];
    if let Some(e) = &args.env {
        query.push(("environment", e.clone()));
    }
    let path = format!("/organizations/{}/monitors/{}/checkins/", org, args.slug);
    let items = client.paginate(&path, &query, args.max).await?;
    if items.is_empty() {
        return print_empty(format);
    }
    let value = Value::Array(items);
    let cols = [
        Column::new("ID", &["id"]),
        Column::new("STATUS", &["status"]),
        Column::new("STARTED", &["dateCreated"]),
        Column::new("DUR_MS", &["duration"]),
        Column::new("ENV", &["environment"]),
    ];
    print_value(format, &value, &cols)
}

async fn audit(
    args: AuditArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let stale_after = time::parse_duration(&args.stale)?;
    let now = Utc::now();
    let items = fetch_monitors(client, org, sentry_project, None, 100, args.max).await?;

    let mut findings: Vec<Value> = Vec::new();
    let mut push = |slug: &str, finding: &str, detail: String| {
        findings.push(json!({ "slug": slug, "finding": finding, "detail": detail }));
    };

    for m in &items {
        let slug = m.get("slug").and_then(|v| v.as_str()).unwrap_or("?");
        let status = m.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let muted = m.get("isMuted").and_then(|v| v.as_bool()).unwrap_or(false);
        let envs = env_statuses(m);
        let config = m.get("config");

        if status == "disabled" {
            push(slug, "disabled", "monitor is disabled and accepts no check-ins".into());
        }
        for (env, s) in &envs {
            if s == "error" {
                if muted {
                    push(
                        slug,
                        "muted-while-failing",
                        format!("environment '{}' is failing but the monitor is muted", env),
                    );
                } else {
                    push(slug, "failing", format!("environment '{}' status is error", env));
                }
            }
        }
        if envs.is_empty() && status != "disabled" {
            push(slug, "never-checked-in", "no environments; no check-in ever received".into());
        }
        if let Some(last) = env_timestamps(m, "lastCheckIn") {
            if now - last > stale_after {
                push(
                    slug,
                    "stale",
                    format!("last check-in {} (older than {})", time::to_iso(last), args.stale),
                );
            }
        }
        let margin = config.and_then(|c| c.get("checkin_margin"));
        if matches!(margin, None | Some(Value::Null)) {
            push(
                slug,
                "no-checkin-margin",
                "config.checkin_margin unset; missed runs detected only after the 1-minute default".into(),
            );
        }
        let runtime = config.and_then(|c| c.get("max_runtime"));
        if matches!(runtime, None | Some(Value::Null)) {
            push(
                slug,
                "no-max-runtime",
                "config.max_runtime unset; hung jobs fail only after the 30-minute default".into(),
            );
        }
    }

    if findings.is_empty() {
        if !matches!(format, OutputFormat::Json | OutputFormat::Ndjson) {
            eprintln!("Audited {} monitors: no findings.", items.len());
        } else {
            print_empty(format)?;
        }
        return Ok(());
    }
    let n = findings.len();
    let value = Value::Array(findings);
    let cols = [
        Column::new("SLUG", &["slug"]),
        Column::new("FINDING", &["finding"]),
        Column::new("DETAIL", &["detail"]),
    ];
    print_value(format, &value, &cols)?;
    if args.fail_on_findings {
        eprintln!("{} findings across {} monitors.", n, items.len());
        std::process::exit(2);
    }
    Ok(())
}

const INTERVAL_UNITS: &[&str] = &["minute", "hour", "day", "week", "month", "year"];

fn build_payload(args: &CreateArgs, project: Option<&str>) -> Result<Value> {
    if let Some(path) = &args.from_file {
        let raw = if path == "-" {
            use std::io::Read;
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s
        } else {
            std::fs::read_to_string(path)?
        };
        let mut v: Value = serde_json::from_str(&raw)
            .map_err(|e| anyhow!("invalid JSON in {}: {}", path, e))?;
        if v.get("project").is_none() {
            if let (Some(obj), Some(p)) = (v.as_object_mut(), project) {
                obj.insert("project".into(), json!(p));
            }
        }
        return Ok(v);
    }

    let slug = args
        .slug
        .as_deref()
        .ok_or_else(|| anyhow!("a monitor slug is required (or use --from-file)"))?;
    let project = project.ok_or_else(|| {
        anyhow!("monitors create needs a project. Pass -P/--sentry-project <slug>.")
    })?;

    let schedule: (Value, &str) = match (&args.crontab, &args.interval) {
        (Some(c), None) => (json!(c), "crontab"),
        (None, Some(iv)) => {
            let n: u64 = iv[0]
                .parse()
                .map_err(|_| anyhow!("--interval count must be a number, got '{}'", iv[0]))?;
            let unit = iv[1].as_str();
            if !INTERVAL_UNITS.contains(&unit) {
                return Err(anyhow!(
                    "--interval unit must be one of {}, got '{}'",
                    INTERVAL_UNITS.join("|"),
                    unit
                ));
            }
            (json!([n, unit]), "interval")
        }
        _ => {
            return Err(anyhow!(
                "exactly one of --crontab or --interval is required"
            ))
        }
    };

    let mut config = serde_json::Map::new();
    config.insert("schedule_type".into(), json!(schedule.1));
    config.insert("schedule".into(), schedule.0);
    config.insert("timezone".into(), json!(args.timezone));
    if let Some(v) = args.checkin_margin {
        config.insert("checkin_margin".into(), json!(v));
    }
    if let Some(v) = args.max_runtime {
        config.insert("max_runtime".into(), json!(v));
    }
    if let Some(v) = args.failure_threshold {
        config.insert("failure_issue_threshold".into(), json!(v));
    }
    if let Some(v) = args.recovery_threshold {
        config.insert("recovery_threshold".into(), json!(v));
    }

    let mut body = serde_json::Map::new();
    body.insert("project".into(), json!(project));
    body.insert("slug".into(), json!(slug));
    body.insert(
        "name".into(),
        json!(args.name.clone().unwrap_or_else(|| slug.to_string())),
    );
    body.insert("config".into(), Value::Object(config));
    if let Some(o) = &args.owner {
        body.insert("owner".into(), json!(o));
    }
    Ok(Value::Object(body))
}

async fn create(
    args: CreateArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let payload = build_payload(&args, sentry_project)?;

    if !args.yes {
        print_value(format, &payload, &[])?;
        eprintln!("Dry run: nothing created. Re-run with --yes to POST this monitor.");
        return Ok(());
    }

    let url = client.url(&format!("/organizations/{}/monitors/", org));
    let resp = client
        .send(client.request(reqwest::Method::POST, &url).json(&payload))
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Sentry API error ({}): {}", status, text);
    }
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    print_value(format, &value, &[])
}
