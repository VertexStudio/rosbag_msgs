use anyhow::Result;
use clap::{Parser, Subcommand};
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{self, EnvFilter};

mod toolbox;

#[derive(Parser)]
#[command(name = "rosbag_mcp_server")]
#[command(about = "A MCP server for rosbags")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Output the tools JSON schema
    Toolbox,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Toolbox) => {
            // Output only the tools JSON schema, no logging or other output
            let tools_schema = toolbox::Toolbox::get_tools_schema_as_json();
            println!("{}", tools_schema);
            return Ok(());
        }
        None => {
            // Default behavior - start the MCP server
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()),
                )
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .init();

            tracing::info!("Starting MCP server");

            let server = toolbox::Toolbox::new();

            // Create an instance of our MCP server
            let service = server.serve(stdio()).await.inspect_err(|e| {
                tracing::error!("serving error: {:?}", e);
            })?;

            service.waiting().await?;
        }
    }

    Ok(())
}
