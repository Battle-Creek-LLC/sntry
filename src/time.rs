use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};

/// Parses an absolute ISO-8601 timestamp or a relative spec like `now`,
/// `now-15m`, `now-1h`, `now-7d`, `-24h` (sumo-style).
pub fn parse(spec: &str) -> Result<DateTime<Utc>> {
    let s = spec.trim();
    if s == "now" {
        return Ok(Utc::now());
    }
    if let Some(rest) = s.strip_prefix("now-") {
        return Ok(Utc::now() - parse_duration(rest)?);
    }
    if let Some(rest) = s.strip_prefix('-') {
        return Ok(Utc::now() - parse_duration(rest)?);
    }
    // Try ISO-8601 with timezone.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Try naive ISO-8601 → assume UTC.
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
    }
    Err(anyhow!("invalid time spec: {}", spec))
}

pub fn parse_duration(spec: &str) -> Result<Duration> {
    let s = spec.trim();
    if s.is_empty() {
        return Err(anyhow!("empty duration"));
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let n: i64 = num_str
        .parse()
        .map_err(|_| anyhow!("invalid duration: {}", spec))?;
    Ok(match unit {
        "s" => Duration::seconds(n),
        "m" => Duration::minutes(n),
        "h" => Duration::hours(n),
        "d" => Duration::days(n),
        "w" => Duration::weeks(n),
        _ => return Err(anyhow!("unknown duration unit '{}' in '{}'", unit, spec)),
    })
}

pub fn to_iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
