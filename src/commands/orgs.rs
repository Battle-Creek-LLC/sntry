use anyhow::Result;
use clap::{Args, Subcommand};

use crate::http::ApiClient;
use crate::output::{print_value, Column, OutputFormat};

#[derive(Debug, Args)]
pub struct OrgsArgs {
    #[command(subcommand)]
    pub cmd: OrgsCmd,
}

#[derive(Debug, Subcommand)]
pub enum OrgsCmd {
    /// List organizations accessible to the auth token.
    List,
}

pub async fn run(args: OrgsArgs, client: &ApiClient, format: OutputFormat) -> Result<()> {
    match args.cmd {
        OrgsCmd::List => list(client, format).await,
    }
}

async fn list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let items = client.paginate("/organizations/", &[], 0).await?;
    if items.is_empty() {
        return crate::output::print_empty(format);
    }
    let value = serde_json::Value::Array(items);
    let cols = [
        Column::new("SLUG", &["slug"]),
        Column::new("NAME", &["name"]),
        Column::new("ROLE", &["role"]),
    ];
    print_value(format, &value, &cols)
}
