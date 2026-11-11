use std::process::ExitCode;

use clap::Parser;
use rustyhgm_coding_harness::harness::{Args, run};

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rustyhgm-coding-harness: {e}");
            ExitCode::FAILURE
        }
    }
}
