use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

use crate::filter;
use crate::share;

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
    pub shares: Option<Vec<share::ConfigShare>>,
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

#[derive(Debug, Deserialize, Default)]
pub struct FilterConfig {
    pub allowed_types: Option<Vec<String>>,
    pub blocked_types: Option<Vec<String>>,
    pub allowed_patterns: Option<Vec<String>>,
    pub blocked_patterns: Option<Vec<String>>,
    pub min_size: Option<String>,
    pub max_size: Option<String>,
}

impl FilterConfig {
    pub fn to_filter_chain(&self) -> Result<filter::FilterChain, String> {
        let mut chain = filter::FilterChain::new();

        if let Some(types) = &self.allowed_types {
            if !types.is_empty() {
                chain.add(Box::new(filter::TypeFilter::new(types.clone(), vec![])));
            }
        }

        if let Some(min) = &self.min_size {
            let min_bytes = filter::parse_size(min)?;
            chain.add(Box::new(filter::SizeFilter::new(Some(min_bytes), None)));
        }
        if let Some(max) = &self.max_size {
            let max_bytes = filter::parse_size(max)?;
            chain.add(Box::new(filter::SizeFilter::new(None, Some(max_bytes))));
        }

        if let Some(patterns) = &self.allowed_patterns {
            if !patterns.is_empty() {
                chain.add(Box::new(filter::PatternFilter::new(patterns.clone())));
            }
        }

        Ok(chain)
    }
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

    pub fn config_shares(&self) -> &[share::ConfigShare] {
        self.shares.as_deref().unwrap_or(&[])
    }
}
