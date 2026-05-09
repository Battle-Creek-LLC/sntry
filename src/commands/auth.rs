use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use dialoguer::{Input, Password};

use crate::config::{self, Profile};

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub cmd: AuthCmd,
}

#[derive(Debug, Subcommand)]
pub enum AuthCmd {
    /// Store credentials for a profile.
    Login(LoginArgs),
    /// Remove credentials for a profile (or all profiles).
    Logout(LogoutArgs),
    /// Set the active profile.
    Use(UseArgs),
    /// List configured profiles.
    List,
    /// Show the current authentication state.
    Status,
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub host: Option<String>,
    #[arg(long)]
    pub org: Option<String>,
    #[arg(long)]
    pub token: Option<String>,
}

#[derive(Debug, Args)]
pub struct LogoutArgs {
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct UseArgs {
    pub name: String,
}

pub fn run(args: AuthArgs, config_path: &std::path::Path) -> Result<()> {
    match args.cmd {
        AuthCmd::Login(a) => login(a, config_path),
        AuthCmd::Logout(a) => logout(a, config_path),
        AuthCmd::Use(a) => use_profile(a, config_path),
        AuthCmd::List => list(config_path),
        AuthCmd::Status => status(config_path),
    }
}

fn login(args: LoginArgs, path: &std::path::Path) -> Result<()> {
    let mut cfg = config::load(path).unwrap_or_default();

    let profile_name = match args.profile {
        Some(p) => p,
        None => Input::<String>::new()
            .with_prompt("Profile name")
            .default("default".into())
            .interact_text()?,
    };

    let host = match args.host {
        Some(h) => h,
        None => Input::<String>::new()
            .with_prompt("Sentry host")
            .default("sentry.io".into())
            .interact_text()?,
    };

    let org = match args.org {
        Some(o) => Some(o),
        None => {
            let v: String = Input::<String>::new()
                .with_prompt("Default organization slug (leave empty to skip)")
                .allow_empty(true)
                .interact_text()?;
            if v.trim().is_empty() {
                None
            } else {
                Some(v.trim().to_string())
            }
        }
    };

    let token = match args.token {
        Some(t) if t == "-" => {
            use std::io::Read;
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s.trim().to_string()
        }
        Some(t) => t,
        None => {
            println!(
                "Create a token at: https://{}/settings/account/api/auth-tokens/?name=sntry",
                host
            );
            println!("Then paste it below.");
            Password::new().with_prompt("Auth token").interact()?
        }
    };

    cfg.profiles.insert(
        profile_name.clone(),
        Profile {
            host,
            org,
            auth_token: token,
        },
    );
    if cfg.default_profile.is_none() {
        cfg.default_profile = Some(profile_name.clone());
    }
    config::save(path, &cfg)?;
    println!("Wrote profile '{}' to {}", profile_name, path.display());
    Ok(())
}

fn logout(args: LogoutArgs, path: &std::path::Path) -> Result<()> {
    let mut cfg = config::load(path)?;
    if args.all {
        cfg.profiles.clear();
        cfg.default_profile = None;
        if config::delete_if_empty(path, &cfg)? {
            println!("Removed all profiles ({} deleted).", path.display());
        } else {
            config::save(path, &cfg)?;
            println!("Removed all profiles.");
        }
        return Ok(());
    }
    let target = args
        .profile
        .or_else(|| cfg.default_profile.clone())
        .context("No profile specified and no active profile set.")?;
    if cfg.profiles.remove(&target).is_none() {
        anyhow::bail!("Profile '{}' not found.", target);
    }
    if cfg.default_profile.as_deref() == Some(target.as_str()) {
        cfg.default_profile = cfg.profiles.keys().next().cloned();
    }
    if !config::delete_if_empty(path, &cfg)? {
        config::save(path, &cfg)?;
    }
    println!("Removed profile '{}'.", target);
    Ok(())
}

fn use_profile(args: UseArgs, path: &std::path::Path) -> Result<()> {
    let mut cfg = config::load(path)?;
    if !cfg.profiles.contains_key(&args.name) {
        anyhow::bail!(
            "Profile '{}' not found. Run 'sntry auth list' to see available profiles.",
            args.name
        );
    }
    cfg.default_profile = Some(args.name.clone());
    config::save(path, &cfg)?;
    println!("Switched to profile: {}", args.name);
    Ok(())
}

fn list(path: &std::path::Path) -> Result<()> {
    let cfg = config::load(path)?;
    if cfg.profiles.is_empty() {
        eprintln!("No profiles configured. Run 'sntry auth login' to create one.");
        return Ok(());
    }
    let active = cfg.default_profile.as_deref();
    let name_w = cfg.profiles.keys().map(|n| n.len()).max().unwrap_or(8).max(8);
    let host_w = cfg
        .profiles
        .values()
        .map(|p| p.host.len())
        .max()
        .unwrap_or(10)
        .max(10);
    for (name, p) in &cfg.profiles {
        let marker = if Some(name.as_str()) == active { "*" } else { " " };
        let org = p.org.as_deref().unwrap_or("-");
        println!(
            "{} {:<nw$}  {:<hw$}  (org: {})",
            marker,
            name,
            p.host,
            org,
            nw = name_w,
            hw = host_w
        );
    }
    Ok(())
}

fn status(path: &std::path::Path) -> Result<()> {
    let cfg = config::load(path)?;
    let resolved = crate::auth::resolve(&cfg, None, None);
    match resolved {
        Ok(r) => {
            println!("Profile:    {}", r.profile_name);
            println!("Host:       {}", r.host);
            println!(
                "Org:        {}",
                r.org.as_deref().unwrap_or("(none)")
            );
            println!("Auth token: {}", config::mask_token(&r.auth_token));
            println!("Config:     {}", path.display());
        }
        Err(e) => {
            println!("Not authenticated: {}", e);
            println!("Config:     {}", path.display());
        }
    }
    Ok(())
}
