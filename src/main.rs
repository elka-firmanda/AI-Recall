use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

use ai_recall::config::AppConfig;
use ai_recall::mcp;
use ai_recall::storage::markdown::MarkdownStorage;

#[derive(Parser)]
#[command(name = "ai-recall")]
#[command(about = "Self-hosted AI agent memory system with vector search")]
#[command(version)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<String>,

    /// Data directory path
    #[arg(short, long)]
    data_dir: Option<String>,

    /// HTTP server port
    #[arg(short, long)]
    port: Option<u16>,

    /// HTTP server host
    #[arg(short = 'H', long)]
    host: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start HTTP API server
    Serve,
    /// Start MCP server (stdio)
    Mcp,
    /// Initialize data directory
    Init,
    /// Generate/display auth token
    Token,
    /// Run health check
    Health,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(format!("ai_recall={},rmcp=info", log_level))
        .init();

    info!("AI Recall v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let mut config = if let Some(config_path) = &cli.config {
        AppConfig::from_file(config_path)?
    } else {
        AppConfig::load()?
    };

    // Override with CLI args
    if let Some(data_dir) = cli.data_dir {
        config.storage.data_dir = data_dir.into();
    }
    if let Some(port) = cli.port {
        config.server.port = port;
    }
    if let Some(host) = cli.host {
        config.server.host = host;
    }

    match cli.command {
        Commands::Serve => {
            info!("Starting HTTP server on {}:{}", config.server.host, config.server.port);
            mcp::start_http_server(config).await?;
        }
        Commands::Mcp => {
            info!("Starting MCP stdio server");
            mcp::start_stdio_server(config).await?;
        }
        Commands::Init => {
            let data_dir = config.storage.data_dir.clone();
            info!("Initializing data directory at {:?}", data_dir);
            let storage = MarkdownStorage::new(config.storage);
            storage.initialize()?;
            
            // Generate and display auth token if not set
            if config.server.auth_token.is_none() {
                let token = generate_auth_token();
                println!("\n╔══════════════════════════════════════════════════════════╗");
                println!("║  AI Recall - Authentication Token                          ║");
                println!("╠══════════════════════════════════════════════════════════╣");
                println!("║  Token: {}    ║", token);
                println!("║                                                           ║");
                println!("║  Store this securely. Use with:                          ║");
                println!("║    Authorization: Bearer {}    ║", &token[..20]);
                println!("╚══════════════════════════════════════════════════════════╝\n");
                
                // Save token to .env file for reference
                let env_path = data_dir.join(".env");
                std::fs::write(&env_path, format!("AI_RECALL_SERVER_AUTH_TOKEN={}\n", token))?;
                println!("Token saved to {:?}", env_path);
            }
            
            println!("✓ Data directory initialized successfully");
        }
        Commands::Token => {
            if let Some(token) = &config.server.auth_token {
                println!("Current auth token: {}", token);
            } else {
                let token = generate_auth_token();
                println!("Generated auth token: {}", token);
                println!("\nSet this in your environment:");
                println!("  export AI_RECALL_SERVER_AUTH_TOKEN={}", token);
            }
        }
        Commands::Health => {
            // Simple health check
            println!("Checking health...");
            // TODO: Implement actual health check
            println!("✓ Configuration loaded successfully");
        }
    }

    Ok(())
}

fn generate_auth_token() -> String {
    use rand::Rng;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill(&mut bytes);
    format!("arec_{}", URL_SAFE_NO_PAD.encode(&bytes))
}
