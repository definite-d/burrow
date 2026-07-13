use clap::Parser;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod filter;
mod server;
mod share;
mod tunnel;
use tunnel::Tunnel;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "burrow=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = config::Cli::parse();
    let config = config::Config::load(&cli).unwrap_or_else(|e| {
        eprintln!("Error loading config: {e}");
        std::process::exit(1);
    });

    let addr = format!("{}:{}", config.host(), config.port());
    let serve_dir = config.dir();

    let share_store = Arc::new(share::ShareStore::new());

    tracing::info!("Burrow v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Serving: {}", serve_dir.display());
    tracing::info!("Listening on {addr}");

    let mut tunnel_handle: Option<tunnel::spawned::SpawnedTunnel> = None;
    let mut tunnel_url = String::new();

    if config.tunnel_enabled() {
        tracing::info!("Tunnel: enabled (cloudflared)");
        let mut spawned = tunnel::spawned::SpawnedTunnel::new();
        match spawned.start(config.port()).await {
            Ok(url) => {
                tracing::info!("Public URL: {url}");
                tunnel_url = url;
                tunnel_handle = Some(spawned);
            }
            Err(e) => {
                eprintln!("Failed to start tunnel: {e}");
                eprintln!("Starting in local-only mode.");
            }
        }
    } else {
        tracing::info!("Tunnel: disabled");
    }

    let app = server::router(serve_dir, share_store, tunnel_url);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    if let Some(mut tunnel) = tunnel_handle {
        tunnel.stop().await;
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutting down...");
}
