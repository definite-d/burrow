use clap::Parser;

mod config;

fn main() {
    let cli = config::Cli::parse();
    let config = config::Config::load(&cli).unwrap_or_else(|e| {
        eprintln!("Error loading config: {e}");
        std::process::exit(1);
    });

    println!("Burrow v{}", env!("CARGO_PKG_VERSION"));
    println!("Serving: {}", config.dir().display());
    println!("Listening on {}:{}", config.host(), config.port());
    println!("Tunnel: {}", if config.tunnel_enabled() { "enabled" } else { "disabled" });
}
