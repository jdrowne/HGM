use std::process::ExitCode;

use clap::Parser;
use hgm::cli::{Cli, Command};
use hgm::run::{JITTER, evaluate, prepare_polyglot, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|cwd| match &cli.command {
            Command::Run(args) => run(args, &cwd, JITTER),
            Command::Evaluate(args) => evaluate(args, &cwd),
            Command::PreparePolyglot => prepare_polyglot(&cwd),
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hgm: {e}");
            ExitCode::FAILURE
        }
    }
}
