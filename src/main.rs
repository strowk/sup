mod ui;
mod cli;
mod credentials;
mod hooks;
mod pull;
mod sup;
mod serde;

fn main() {
    let cli = cli::Cli::parse();
    if let Err(e) = sup::run_sup(cli.r#continue, cli.abort, cli.version, cli.message, cli.yes, cli.no_verify) {
        println!("Error: {e}");
        std::process::exit(1);
    }
}
