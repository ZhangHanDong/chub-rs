use chub_rs::commands::{self, Cli};
use chub_rs::core;
use clap::Parser;

fn main() {
    let cli = Cli::parse();
    let json = cli.json;

    if cli.cli_version {
        println!("chub {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if cli.command.is_none() {
        commands::print_usage();
        return;
    }

    if let Err(e) = commands::run(cli) {
        core::output::print_error(&format!("{e}"), json);
        std::process::exit(1);
    }
}
