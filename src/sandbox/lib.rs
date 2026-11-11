pub mod docker;
pub mod error;
pub mod ignore;
pub mod source;

pub use docker::{BuildOptions, Container, Docker, ExecOptions, ExecResult, RunOptions};
pub use error::Error;

pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
