use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "burrow", about = "Share folders over the internet")]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "burrow.toml")]
    pub config: PathBuf,

    /// Directory to serve files from
    #[arg(short, long)]
    pub dir: Option<PathBuf>,

    /// Server host
    #[arg(long)]
    pub host: Option<String>,

    /// Server port
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Disable tunnel (local-only mode)
    #[arg(long)]
    pub no_tunnel: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub server: Option<ServerConfig>,
    pub tunnel: Option<TunnelConfig>,
    pub admin: Option<AdminConfig>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ServerConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TunnelConfig {
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AdminConfig {
    pub enabled: Option<bool>,
    pub token: Option<String>,
}

impl Config {
    pub fn load(cli: &Cli) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = if cli.config.exists() {
            let content = std::fs::read_to_string(&cli.config)?;
            toml::from_str(&content)?
        } else {
            Config::default()
        };

        if let Some(dir) = &cli.dir {
            config
                .server
                .get_or_insert_with(ServerConfig::default)
                .dir = Some(dir.clone());
        }
        if let Some(host) = &cli.host {
            config
                .server
                .get_or_insert_with(ServerConfig::default)
                .host = Some(host.clone());
        }
        if let Some(port) = cli.port {
            config
                .server
                .get_or_insert_with(ServerConfig::default)
                .port = Some(port);
        }
        if cli.no_tunnel {
            config
                .tunnel
                .get_or_insert_with(TunnelConfig::default)
                .enabled = Some(false);
        }

        Ok(config)
    }

    pub fn host(&self) -> &str {
        self.server
            .as_ref()
            .and_then(|s| s.host.as_deref())
            .unwrap_or("127.0.0.1")
    }

    pub fn port(&self) -> u16 {
        self.server
            .as_ref()
            .and_then(|s| s.port)
            .unwrap_or(8080)
    }

    pub fn dir(&self) -> PathBuf {
        self.server
            .as_ref()
            .and_then(|s| s.dir.clone())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn tunnel_enabled(&self) -> bool {
        self.tunnel
            .as_ref()
            .and_then(|t| t.enabled)
            .unwrap_or(true)
    }

    pub fn admin_enabled(&self) -> bool {
        self.admin
            .as_ref()
            .and_then(|a| a.enabled)
            .unwrap_or(true)
    }
}
