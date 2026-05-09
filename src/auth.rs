use anyhow::{anyhow, Result};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Resolved {
    pub profile_name: String,
    pub host: String,
    pub org: Option<String>,
    pub auth_token: String,
}

pub fn resolve(
    cfg: &Config,
    cli_profile: Option<&str>,
    cli_org: Option<&str>,
) -> Result<Resolved> {
    let profile_name = cli_profile
        .map(String::from)
        .or_else(|| std::env::var("SNTRY_PROFILE").ok())
        .or_else(|| cfg.default_profile.clone())
        .unwrap_or_else(|| "default".into());

    let from_file = cfg.profiles.get(&profile_name);

    if cli_profile.is_some() && from_file.is_none() {
        return Err(anyhow!(
            "Profile '{}' not found. Run 'sntry auth list' to see available profiles.",
            profile_name
        ));
    }

    let host = std::env::var("SENTRY_HOST")
        .ok()
        .or_else(|| from_file.map(|p| p.host.clone()))
        .unwrap_or_else(|| "sentry.io".into());

    let org = cli_org
        .map(String::from)
        .or_else(|| std::env::var("SENTRY_ORG").ok())
        .or_else(|| from_file.and_then(|p| p.org.clone()));

    let auth_token = std::env::var("SENTRY_AUTH_TOKEN")
        .ok()
        .or_else(|| from_file.map(|p| p.auth_token.clone()))
        .ok_or_else(|| {
            anyhow!("Not authenticated. Run 'sntry auth login' to set up credentials.")
        })?;

    Ok(Resolved {
        profile_name,
        host,
        org,
        auth_token,
    })
}

pub fn require_org(r: &Resolved) -> Result<&str> {
    r.org.as_deref().ok_or_else(|| {
        anyhow!("No organization configured. Pass --org <slug> or set one in your profile.")
    })
}
