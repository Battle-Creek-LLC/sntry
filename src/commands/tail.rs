use anyhow::Result;
use chrono::Utc;
use clap::Args;
use std::collections::HashSet;
use std::time::Duration;

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::OutputFormat;
use crate::time as t;

#[derive(Debug, Args)]
pub struct TailArgs {
    #[arg(default_value = "")]
    pub query: String,
    #[arg(long, default_values_t = ["id".to_string(), "title".to_string(), "timestamp".to_string()])]
    pub field: Vec<String>,
    #[arg(long, default_value = "5s")]
    pub interval: String,
    #[arg(long, default_value = "now")]
    pub since: String,
    #[arg(long, default_value = "errors")]
    pub dataset: String,
    #[arg(long)]
    pub environment: Option<String>,
}

pub async fn run(
    args: TailArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    let org = require_org(auth)?;
    let mut interval = t::parse_duration(&args.interval)?
        .to_std()
        .unwrap_or(Duration::from_secs(5));
    if interval < Duration::from_secs(2) {
        interval = Duration::from_secs(2);
    }
    let mut last = t::parse(&args.since)?;
    let mut seen: HashSet<String> = HashSet::new();
    let path = format!("/organizations/{}/events/", org);
    let url = client.url(&path);

    loop {
        let now = Utc::now();
        let mut q: Vec<(&str, String)> = vec![
            ("query", args.query.clone()),
            ("start", t::to_iso(last)),
            ("end", t::to_iso(now)),
            ("sort", "-timestamp".into()),
            ("per_page", "100".into()),
            ("dataset", args.dataset.clone()),
        ];
        for f in &args.field {
            q.push(("field", f.clone()));
        }
        if let Some(env) = &args.environment {
            q.push(("environment", env.clone()));
        }
        if let Some(proj) = sentry_project {
            q.push(("project", proj.to_string()));
        }

        let resp = client
            .send(client.request(reqwest::Method::GET, &url).query(&q))
            .await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            eprintln!("Sentry API error ({}): {}", status, text);
        } else if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(rows) = v.get("data").and_then(|d| d.as_array()) {
                for row in rows.iter().rev() {
                    let id = row
                        .get("id")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !id.is_empty() && seen.contains(&id) {
                        continue;
                    }
                    if !id.is_empty() {
                        seen.insert(id);
                    }
                    emit(format, row);
                }
            }
        }
        last = now;
        tokio::select! {
            _ = tokio::time::sleep(interval) => {},
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\n{} unique events seen.", seen.len());
                return Ok(());
            }
        }
    }
}

fn emit(format: OutputFormat, row: &serde_json::Value) {
    match format {
        OutputFormat::Json | OutputFormat::Ndjson => {
            let _ = serde_json::to_writer(std::io::stdout().lock(), row);
            println!();
        }
        OutputFormat::Text | OutputFormat::Table => {
            let ts = row.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let title = row.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
            println!("{}  {}  {}", ts, id, title);
        }
    }
}
