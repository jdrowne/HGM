pub mod agent;
pub mod eval;
pub mod git;
pub mod llm;
pub mod parsers;
pub mod patch;
pub mod testrepo;
pub mod tools;
pub mod withtools;

pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
