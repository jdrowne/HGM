use std::process::ExitCode;

use clap::Parser;
use hgm_agent::agent::{Args, run};

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hgm-agent: {e}");
            ExitCode::FAILURE
        }
    }
}
