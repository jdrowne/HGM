use std::error::Error as _;
use std::io;

use rustyhgm_sandbox::Error;

fn command(code: Option<i32>) -> Error {
    Error::Command {
        args: vec!["docker".into(), "ps".into()],
        code,
        output: " boom \n".into(),
    }
}

fn cases() -> Vec<(Error, &'static str)> {
    vec![
        (Error::from(io::Error::other("disk")), "disk"),
        (Error::Pattern("[".into()), "Invalid git pattern: \"[\""),
        (
            Error::Quoting("'a".into()),
            "Unbalanced quotes in command: \"'a\"",
        ),
        (
            command(Some(2)),
            "`docker ps` failed with exit code 2: boom",
        ),
        (command(None), "`docker ps` failed with a signal: boom"),
        (Error::ExitCode(3), "Script failed with exit code 3"),
        (Error::NotFound("x".into()), "x not found"),
    ]
}

#[test]
fn every_error_says_what_went_wrong() {
    let (actual, expected): (Vec<String>, Vec<String>) = cases()
        .into_iter()
        .map(|(error, text)| (error.to_string(), text.to_owned()))
        .unzip();
    assert_eq!(actual, expected);
}

#[test]
fn only_io_errors_have_a_source() {
    let (actual, expected): (Vec<bool>, Vec<bool>) = cases()
        .into_iter()
        .map(|(error, _)| (error.source().is_some(), matches!(error, Error::Io(_))))
        .unzip();
    assert_eq!(actual, expected);
}
