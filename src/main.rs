mod cli;
mod sup;
mod pull;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = sup::run_sup(cli.r#continue, cli.abort) {
        tracing::error!("Error: {e}");
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
