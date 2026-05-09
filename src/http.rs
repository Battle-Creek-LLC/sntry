use anyhow::{bail, Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT},
    Client, Method, RequestBuilder, Response, StatusCode,
};
use serde::de::DeserializeOwned;
use std::time::Duration;
use tracing::debug;

pub struct ApiClient {
    client: Client,
    pub base: String,
}

impl ApiClient {
    pub fn new(host: &str, token: &str) -> Result<Self> {
        let base = format!("https://{}/api/0", host.trim_end_matches('/'));
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", token);
        let mut auth_value =
            HeaderValue::from_str(&bearer).context("invalid characters in auth token")?;
        auth_value.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth_value);
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("sntry/", env!("CARGO_PKG_VERSION"))),
        );

        let timeout_secs: u64 = std::env::var("SENTRY_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;
        Ok(Self { client, base })
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.client.request(method, url)
    }

    pub async fn send(&self, req: RequestBuilder) -> Result<Response> {
        send_with_retry(req).await
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let url = self.url(path);
        let mut req = self.client.get(&url);
        if !query.is_empty() {
            req = req.query(query);
        }
        let resp = send_with_retry(req).await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(api_error(status, &text));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("could not parse JSON response from {}", url))
    }

    /// Auto-paginate by following the `Link` header. Stops at `max` items
    /// (0 means unlimited).
    pub async fn paginate(
        &self,
        path: &str,
        query: &[(&str, String)],
        max: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let initial_url = self.url(path);
        let mut next_url: Option<String> = None;
        let mut out: Vec<serde_json::Value> = Vec::new();

        loop {
            let req = if let Some(u) = next_url.take() {
                self.client.get(u)
            } else {
                let mut r = self.client.get(&initial_url);
                if !query.is_empty() {
                    r = r.query(query);
                }
                r
            };
            let resp = send_with_retry(req).await?;
            let status = resp.status();
            let link_header = resp
                .headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let text = resp.text().await?;
            if !status.is_success() {
                return Err(api_error(status, &text));
            }
            let page: serde_json::Value = serde_json::from_str(&text)
                .with_context(|| format!("could not parse JSON page from {}", path))?;
            let arr = match page {
                serde_json::Value::Array(a) => a,
                serde_json::Value::Object(_) => vec![page],
                other => bail!("unexpected JSON shape from {}: {}", path, other),
            };
            for item in arr {
                out.push(item);
                if max != 0 && out.len() >= max {
                    return Ok(out);
                }
            }
            if let Some(next) = parse_next_link(link_header.as_deref()) {
                next_url = Some(next);
            } else {
                break;
            }
        }
        Ok(out)
    }
}

async fn send_with_retry(req: RequestBuilder) -> Result<Response> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let cloned = req
            .try_clone()
            .context("cannot retry a streaming-body request")?;
        let res = cloned.send().await;
        match res {
            Ok(resp) => {
                let status = resp.status();
                if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                    && attempt <= 3
                {
                    let delay = retry_after(&resp).unwrap_or_else(|| {
                        Duration::from_millis(500u64 * (1 << (attempt - 1)))
                    });
                    debug!(?status, ?delay, attempt, "retrying");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                if attempt <= 1 {
                    debug!(error = %e, "network error, retrying once");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                return Err(e).context("network error");
            }
        }
    }
}

fn retry_after(resp: &Response) -> Option<Duration> {
    let v = resp.headers().get("retry-after")?.to_str().ok()?;
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    None
}

fn parse_next_link(link: Option<&str>) -> Option<String> {
    let s = link?;
    for part in s.split(',') {
        let part = part.trim();
        if part.contains("rel=\"next\"") && part.contains("results=\"true\"") {
            let start = part.find('<')? + 1;
            let end = part[start..].find('>')? + start;
            return Some(part[start..end].to_string());
        }
    }
    None
}

#[allow(dead_code)]
pub fn exit_code_for_status(status: StatusCode) -> i32 {
    match status.as_u16() {
        401 | 403 => 2,
        404 => 3,
        429 => 4,
        500..=599 => 5,
        _ => 1,
    }
}

fn api_error(status: StatusCode, body: &str) -> anyhow::Error {
    let msg = if body.is_empty() {
        status.to_string()
    } else {
        match serde_json::from_str::<serde_json::Value>(body) {
            Ok(v) => v
                .get("detail")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    v.get("error")
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| body.to_string()),
            Err(_) => body.to_string(),
        }
    };
    anyhow::anyhow!("Sentry API error ({}): {}", status, msg)
}
