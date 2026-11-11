use std::error::Error as _;
use std::io;

use hgm_sandbox::Error;

#[test]
fn every_error_says_what_went_wrong() {
    let command = |code| Error::Command {
        args: vec!["docker".into(), "ps".into()],
        code,
        output: " boom \n".into(),
    };
    let cases = [
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
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
        assert_eq!(error.source().is_some(), matches!(error, Error::Io(_)));
    }
}
