use clap::Parser;
use ludis::{run, Cli};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    install_tracing(&cli.log);

    if let Err(err) = run(cli).await {
        tracing::error!("{err}");
        std::process::exit(1);
    }
}

pub fn install_tracing(level: &str) {
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .init();
}
