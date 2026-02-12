use std::sync::Arc;

use clap::Parser;
use lanpaste::{
    config::{Cli, Commands},
    http, preflight,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cfg = match cli.command {
        Commands::Serve(cmd) => cmd,
    };

    if let Err(err) = preflight::run_preflight(&cfg) {
        eprintln!("{err:?}");
        std::process::exit(1);
    }

    let state = Arc::new(match preflight::build_state(cfg) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    });

    if let Err(err) = http::run_server(state).await {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
