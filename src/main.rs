mod auth;
mod commands;
mod config;
mod http;
mod output;
mod time;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use crate::http::ApiClient;
use crate::output::{resolve_output, OutputFormat};

#[derive(Parser, Debug)]
#[command(name = "sntry", version, about = "Read-side CLI for Sentry")]
struct Cli {
    /// Profile name from the TOML config.
    #[arg(long, short = 'p', global = true, env = "SNTRY_PROFILE")]
    profile: Option<String>,

    /// Path to the TOML config file.
    #[arg(long, global = true, env = "SNTRY_CONFIG")]
    config: Option<PathBuf>,

    /// Override the org slug for a single command.
    #[arg(long, short = 'O', global = true, env = "SENTRY_ORG")]
    org: Option<String>,

    /// Restrict queries to a Sentry project slug.
    #[arg(long = "sentry-project", short = 'P', global = true, env = "SENTRY_PROJECT")]
    sentry_project: Option<String>,

    /// Output format.
    #[arg(long, short = 'o', global = true, value_enum)]
    output: Option<OutputFormat>,

    /// Suppress progress / status output.
    #[arg(long, short = 'q', global = true)]
    quiet: bool,

    /// Increase verbosity. -v debug, -vv trace.
    #[arg(long, short = 'v', global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage credentials.
    Auth(commands::auth::AuthArgs),
    /// Inspect the TOML config.
    Config(commands::config::ConfigArgs),
    /// Organizations.
    Orgs(commands::orgs::OrgsArgs),
    /// Sentry projects (the resource).
    Projects(commands::projects::ProjectsArgs),
    /// Issues.
    Issues(commands::issues::IssuesArgs),
    /// Events.
    Events(commands::events::EventsArgs),
    /// Releases.
    Releases(commands::releases::ReleasesArgs),
    /// Discover events query.
    Discover(commands::discover::DiscoverArgs),
    /// Stream new events matching a query.
    Tail(commands::tail::TailArgs),
}

fn init_tracing(verbose: u8, quiet: bool) {
    let level = match (verbose, quiet) {
        (_, true) => "error",
        (0, _) => "warn",
        (1, _) => "info",
        (2, _) => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn main() {
    let code = match real_main() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {:#}", e);
            1
        }
    };
    std::process::exit(code);
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose, cli.quiet);

    let config_path = match &cli.config {
        Some(p) => p.clone(),
        None => config::default_path()?,
    };

    // Local-only commands first (no network, no auth).
    if matches!(cli.command, Command::Auth(_) | Command::Config(_)) {
        return match cli.command {
            Command::Auth(args) => commands::auth::run(args, &config_path),
            Command::Config(args) => commands::config::run(args, &config_path),
            _ => unreachable!(),
        };
    }

    // Everything else needs auth + an HTTP client + tokio.
    let cfg = config::load(&config_path)?;
    let resolved = auth::resolve(&cfg, cli.profile.as_deref(), cli.org.as_deref())?;
    let client = ApiClient::new(&resolved.host, &resolved.auth_token)?;
    let format = resolve_output(cli.output, cfg.default_output.as_deref());
    let sentry_project = cli.sentry_project.clone();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let proj = sentry_project.as_deref();
        match cli.command {
            Command::Auth(_) | Command::Config(_) => unreachable!(),
            Command::Orgs(args) => commands::orgs::run(args, &client, format).await,
            Command::Projects(args) => {
                commands::projects::run(args, &client, &resolved, format).await
            }
            Command::Issues(args) => {
                commands::issues::run(args, &client, &resolved, proj, format).await
            }
            Command::Events(args) => {
                commands::events::run(args, &client, &resolved, proj, format).await
            }
            Command::Releases(args) => {
                commands::releases::run(args, &client, &resolved, proj, format).await
            }
            Command::Discover(args) => {
                commands::discover::run(args, &client, &resolved, proj, format).await
            }
            Command::Tail(args) => {
                commands::tail::run(args, &client, &resolved, proj, format).await
            }
        }
    })
}
