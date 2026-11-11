use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("hgm {}", env!("CARGO_PKG_VERSION"));
            println!(
                "crates: {} {} {}",
                hgm_core::CRATE_NAME,
                hgm_sandbox::CRATE_NAME,
                hgm_bench::CRATE_NAME
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("hgm: not implemented yet; the Python hgm.py is still the entry point");
            ExitCode::from(2)
        }
    }
}
