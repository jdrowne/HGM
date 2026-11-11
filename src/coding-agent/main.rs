use std::process::ExitCode;

use clap::Parser;
use rustyhgm_coding_agent::agent::{Args, run};

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rustyhgm-coding-agent: {e}");
            ExitCode::FAILURE
        }
    }
}
