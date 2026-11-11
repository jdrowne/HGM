use std::process::ExitCode;

use clap::Parser;
use rustyhgm::cli::Cli;
use rustyhgm::run::{JITTER, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|cwd| run(&cli, &cwd, JITTER));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rustyhgm: {e}");
            ExitCode::FAILURE
        }
    }
}
