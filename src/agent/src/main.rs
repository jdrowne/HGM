//! `hgm-agent`: the coding agent. It will take the same flags as
//! `coding_agent.py`, with `--polyglot` replacing the file swap.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("hgm-agent {}", env!("CARGO_PKG_VERSION"));
            println!("crates: {}", hgm_agent::CRATE_NAME);
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "hgm-agent: not implemented yet; the Python coding_agent.py is still the agent"
            );
            ExitCode::from(2)
        }
    }
}
