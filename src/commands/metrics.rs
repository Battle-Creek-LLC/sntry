use anyhow::Result;
use clap::{Args, Subcommand};

use crate::auth::{require_org, Resolved};
use crate::http::ApiClient;
use crate::output::{print_empty, print_value, OutputFormat};
use crate::time;

#[derive(Debug, Args)]
pub struct MetricsArgs {
    #[command(subcommand)]
    pub cmd: MetricsCmd,
}

#[derive(Debug, Subcommand)]
pub enum MetricsCmd {
    /// Query the Trace Metrics (Application Metrics) dataset.
    Query(QueryArgs),
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Metric name, e.g. http_cache.hit or gemini.cost_usd.
    pub name: String,
    /// Aggregation. counters: sum; distributions also: avg / min / max / p50..p99.
    /// One of: sum avg count count_unique min max p50 p75 p90 p95 p99.
    #[arg(long, default_value = "sum")]
    pub stat: String,
    /// Metric type: counter | gauge | distribution. Auto-detected from the name if omitted.
    #[arg(long = "type")]
    pub metric_type: Option<String>,
    /// Metric unit (e.g. millisecond, byte, usd, none). Auto-detected from the name if omitted.
    #[arg(long)]
    pub unit: Option<String>,
    /// Group results by an attribute (repeatable), e.g. --group-by org_id.
    #[arg(long = "group-by")]
    pub group_by: Vec<String>,
    /// Extra attribute filter in Sentry search syntax, e.g. 'workflow:checkout'.
    #[arg(long, default_value = "")]
    pub query: String,
    #[arg(long, short = 'f', default_value = "now-24h")]
    pub from: String,
    #[arg(long, short = 't', default_value = "now")]
    pub to: String,
    /// Sort spec (prefix - for descending). Defaults to the aggregate, descending.
    #[arg(long)]
    pub sort: Option<String>,
    #[arg(long, short = 'n', default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value_t = 1000)]
    pub max: usize,
    #[arg(long)]
    pub environment: Option<String>,
    /// Dataset to query. Defaults to the Trace Metrics dataset.
    #[arg(long, default_value = "tracemetrics")]
    pub dataset: String,
}

pub async fn run(
    args: MetricsArgs,
    client: &ApiClient,
    auth: &Resolved,
    sentry_project: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    match args.cmd {
        MetricsCmd::Query(a) => query(a, client, auth, sentry_project, format).await,
    }
}

/// Builds the trace-metrics aggregate field, e.g. `sum(value,http_cache.hit,counter,none)`.
/// Sentry requires the full (name, type, unit) triple for trace-metric aggregates.
fn aggregate_field(stat: &str, name: &str, metric_type: &str, unit: &str) -> String {
    format!("{}(value,{},{},{})", stat, name, metric_type, unit)
}

/// Looks up a metric's `type` and `unit` by name against the tracemetrics dataset.
/// Counters report a null unit, which the aggregate function expects as the literal
/// `none`; distributions already report `none`. Mirrors what the Sentry UI does to
/// populate its metric picker.
async fn resolve_type_unit(
    client: &ApiClient,
    org: &str,
    name: &str,
    from: &str,
    to: &str,
    dataset: &str,
    environment: Option<&str>,
    sentry_project: Option<&str>,
) -> Result<(String, String)> {
    let mut q: Vec<(&str, String)> = vec![
        ("query", format!("metric.name:{}", name)),
        ("start", from.to_string()),
        ("end", to.to_string()),
        ("dataset", dataset.to_string()),
        ("field", "metric.name".to_string()),
        ("field", "metric.type".to_string()),
        ("field", "metric.unit".to_string()),
        ("field", "count(metric.name)".to_string()),
        ("sort", "-count(metric.name)".to_string()),
        ("per_page", "25".to_string()),
    ];
    if let Some(env) = environment {
        q.push(("environment", env.to_string()));
    }
    if let Some(proj) = sentry_project {
        q.push(("project", proj.to_string()));
    }

    let path = format!("/organizations/{}/events/", org);
    let url = client.url(&path);
    let resp = client.send(client.request(reqwest::Method::GET, &url).query(&q)).await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Sentry API error ({}): {}", status, text);
    }
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let rows = value
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    // Rows are ordered by descending sample count; take the dominant exact-name match.
    let row = rows
        .iter()
        .find(|r| r.get("metric.name").and_then(|v| v.as_str()) == Some(name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No trace metric named '{}' found in the selected time range. \
                 Check the name or widen --from, or pin --type/--unit explicitly.",
                name
            )
        })?;
    let metric_type = row
        .get("metric.type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let unit = match row.get("metric.unit").and_then(|v| v.as_str()) {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => "none".to_string(),
    };
    Ok((metric_type, unit))
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

    // Trace-metric aggregates require the full (name, type, unit) triple. Resolve any
    // missing piece from the metric name; explicit --type/--unit always win.
    let (metric_type, unit) = match (args.metric_type.clone(), args.unit.clone()) {
        (Some(t), Some(u)) => (t, u),
        (t_opt, u_opt) => {
            let (rt, ru) = resolve_type_unit(
                client,
                org,
                &args.name,
                &from,
                &to,
                &args.dataset,
                args.environment.as_deref(),
                sentry_project,
            )
            .await?;
            (t_opt.unwrap_or(rt), u_opt.unwrap_or(ru))
        }
    };
    tracing::debug!(name = %args.name, %metric_type, %unit, "resolved trace metric");

    let agg = aggregate_field(&args.stat, &args.name, &metric_type, &unit);
    let sort = args.sort.clone().unwrap_or_else(|| format!("-{}", agg));

    let mut query: Vec<(&str, String)> = vec![
        ("query", args.query.clone()),
        ("start", from),
        ("end", to),
        ("sort", sort),
        ("per_page", args.limit.to_string()),
        ("dataset", args.dataset.clone()),
    ];
    // Group-by attributes are selected as plain fields (ahead of the aggregate so
    // they lead each result row); selecting an aggregate alongside them groups by them.
    for g in &args.group_by {
        query.push(("field", g.clone()));
    }
    query.push(("field", agg.clone()));
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

    // Like Discover, the events endpoint wraps rows under `data`.
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

#[cfg(test)]
mod tests {
    use super::aggregate_field;

    #[test]
    fn aggregate_counter() {
        assert_eq!(
            aggregate_field("sum", "http_cache.hit", "counter", "none"),
            "sum(value,http_cache.hit,counter,none)"
        );
    }

    #[test]
    fn aggregate_distribution_percentile() {
        assert_eq!(
            aggregate_field("p90", "gemini.cost_usd", "distribution", "none"),
            "p90(value,gemini.cost_usd,distribution,none)"
        );
    }

    #[test]
    fn aggregate_with_real_unit() {
        assert_eq!(
            aggregate_field("avg", "request.duration", "distribution", "millisecond"),
            "avg(value,request.duration,distribution,millisecond)"
        );
    }
}
