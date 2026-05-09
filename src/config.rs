use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_output: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    pub auth_token: String,
}

pub fn default_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SNTRY_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        let home = std::env::var("HOME").context("HOME is not set")?;
        PathBuf::from(home).join(".config")
    };
    Ok(base.join("sntry").join("config.toml"))
}

pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    check_mode(path)?;
    let s = fs::read_to_string(path)
        .with_context(|| format!("Unable to read {}", path.display()))?;
    let cfg: Config = toml::from_str(&s)
        .with_context(|| format!("Invalid TOML in {}", path.display()))?;
    Ok(cfg)
}

pub fn save(path: &Path, cfg: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(cfg)?;
    fs::write(path, s)?;
    restrict_mode(path);
    Ok(())
}

#[cfg(unix)]
fn check_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path)
        .with_context(|| format!("Unable to read {}", path.display()))?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        anyhow::bail!(
            "Refusing to read {}: file mode {:04o}; run 'chmod 600 {}'",
            path.display(),
            mode,
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_mode(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_mode(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_mode(_path: &Path) {}

pub fn delete_if_empty(path: &Path, cfg: &Config) -> Result<bool> {
    if cfg.profiles.is_empty() && cfg.default_profile.is_none() && cfg.default_output.is_none() {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(true);
    }
    Ok(false)
}

pub fn mask_token(token: &str) -> String {
    let len = token.chars().count();
    if len <= 8 {
        return "*".repeat(len);
    }
    let prefix: String = token.chars().take(8).collect();
    let suffix: String = token.chars().rev().take(4).collect::<String>().chars().rev().collect();
    format!("{}…{}", prefix, suffix)
}
