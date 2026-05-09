use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::{self, mask_token};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub cmd: ConfigCmd,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Print the absolute path of the config file.
    Path,
    /// Print the resolved config (token masked).
    Show {
        #[arg(long)]
        profile: Option<String>,
    },
}

pub fn run(args: ConfigArgs, path: &std::path::Path) -> Result<()> {
    match args.cmd {
        ConfigCmd::Path => {
            println!("{}", path.display());
            Ok(())
        }
        ConfigCmd::Show { profile } => show(path, profile.as_deref()),
    }
}

fn show(path: &std::path::Path, profile: Option<&str>) -> Result<()> {
    let mut cfg = config::load(path)?;
    for p in cfg.profiles.values_mut() {
        p.auth_token = mask_token(&p.auth_token);
    }
    if let Some(name) = profile {
        let p = cfg
            .profiles
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found.", name))?;
        let v = serde_json::to_value(p)?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &v)?;
        println!();
        return Ok(());
    }
    let s = toml::to_string_pretty(&cfg)?;
    print!("{}", s);
    Ok(())
}
