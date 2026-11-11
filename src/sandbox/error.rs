use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Pattern(String),
    Quoting(String),
    Command {
        args: Vec<String>,
        code: Option<i32>,
        output: String,
    },
    ExitCode(i32),
    NotFound(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Pattern(p) => write!(f, "Invalid git pattern: {p:?}"),
            Error::Quoting(c) => write!(f, "Unbalanced quotes in command: {c:?}"),
            Error::Command { args, code, output } => {
                let code = code.map_or_else(|| "a signal".to_owned(), |c| format!("exit code {c}"));
                write!(
                    f,
                    "`{}` failed with {code}: {}",
                    args.join(" "),
                    output.trim()
                )
            }
            Error::ExitCode(code) => write!(f, "Script failed with exit code {code}"),
            Error::NotFound(what) => write!(f, "{what} not found"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
