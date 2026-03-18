mod handler;
mod loops;
mod server;
mod state;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mxr", about = "Terminal email client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon explicitly
    Daemon {
        /// Run in foreground (for debugging / systemd)
        #[arg(long)]
        foreground: bool,
    },
    /// Trigger sync
    Sync {
        #[arg(long)]
        account: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let is_foreground = matches!(cli.command, Some(Commands::Daemon { foreground: true }));
    init_tracing(is_foreground)?;

    match cli.command {
        Some(Commands::Daemon { .. }) => {
            crate::server::run_daemon().await?;
        }
        Some(Commands::Sync { .. }) => {
            todo!("CLI sync command");
        }
        None => {
            crate::server::ensure_daemon_running().await?;
            mxr_tui::run().await?;
        }
    }

    Ok(())
}

fn init_tracing(foreground: bool) -> anyhow::Result<()> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "mxr=info".parse().unwrap());

    let log_dir = state::AppState::data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("mxr.log"))?;

    let file_layer = fmt::layer().with_writer(file).with_ansi(false);

    if foreground {
        let stdout_layer = fmt::layer().with_writer(std::io::stdout);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(stdout_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .init();
    }

    Ok(())
}
