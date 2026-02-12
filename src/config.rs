use clap::{Parser, Subcommand, ValueEnum};
use ipnet::IpNet;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "lanpaste")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Serve(ServeCmd),
}

#[derive(Debug, Clone, Parser)]
pub struct ServeCmd {
    #[arg(long)]
    pub dir: PathBuf,
    #[arg(long, default_value = "0.0.0.0:8090")]
    pub bind: SocketAddr,
    #[arg(long)]
    pub token: Option<String>,
    #[arg(long, default_value_t = 1_048_576)]
    pub max_bytes: usize,
    #[arg(long, default_value = "off")]
    pub push: PushMode,
    #[arg(long, default_value = "origin")]
    pub remote: String,
    #[arg(long)]
    pub allow_cidr: Vec<IpNet>,
    #[arg(long, default_value = "LAN Paste")]
    pub git_author_name: String,
    #[arg(long, default_value = "paste@lan")]
    pub git_author_email: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum PushMode {
    Off,
    BestEffort,
    Strict,
}

impl std::fmt::Display for PushMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PushMode::Off => write!(f, "off"),
            PushMode::BestEffort => write!(f, "best_effort"),
            PushMode::Strict => write!(f, "strict"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults() {
        let cli = Cli::try_parse_from(["lanpaste", "serve", "--dir", "/tmp/x"]).expect("parse");
        let Commands::Serve(cmd) = cli.command;
        assert_eq!(cmd.bind, "0.0.0.0:8090".parse().expect("bind"));
        assert_eq!(cmd.max_bytes, 1_048_576);
        assert_eq!(cmd.push, PushMode::Off);
        assert_eq!(cmd.remote, "origin");
        assert_eq!(cmd.git_author_name, "LAN Paste");
        assert_eq!(cmd.git_author_email, "paste@lan");
    }
}
