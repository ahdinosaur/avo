use clap::Parser;
use ludis::{install_tracing, run, Cli};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    install_tracing(&cli.log);

    if let Err(err) = run(cli).await {
        tracing::error!("{err}");
        std::process::exit(1);
    }
}
