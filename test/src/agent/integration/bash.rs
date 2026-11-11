use std::path::PathBuf;
use std::time::{Duration, Instant};

use hgm_agent::tools::{Bash, Tools};
use serde_json::json;

fn bash(command: &str) -> String {
    Tools::default().call("bash", &json!({"command": command}))
}

#[test]
fn simple_and_chained_commands_print_their_output() {
    assert_eq!(bash("echo 'hello world'"), "hello world");
    assert_eq!(bash("echo 'first' && echo 'second'"), "first\nsecond");
    assert_eq!(
        bash("echo 'line1' && echo 'line2' && echo 'line3'"),
        "line1\nline2\nline3"
    );
    assert_eq!(
        bash("printf 'line1\\nline2\\nline3'"),
        "line1\nline2\nline3"
    );
    assert_eq!(bash("TEST_VAR='hello' && echo $TEST_VAR"), "hello");
    assert_eq!(bash("sleep 1 && echo 'done'"), "done");
    assert_eq!(
        bash("echo 'test with spaces and !@#$%^&*()'"),
        "test with spaces and !@#$%^&*()"
    );
}

#[test]
fn large_output_is_returned_whole() {
    let out = bash("for i in {1..100}; do echo \"Line $i\"; done");
    assert!(
        out.starts_with("Line 1\n") && out.ends_with("Line 100"),
        "{out}"
    );
    let big = bash("head -c 300000 /dev/zero | tr '\\0' 'a'");
    assert_eq!(big.len(), 300000);
}

#[test]
fn errors_follow_the_output() {
    let out = bash("ls /nonexistent/directory");
    assert!(out.starts_with("Error:\nls: "), "{out}");
    assert!(out.contains("No such file or directory"), "{out}");
    assert_eq!(bash("echo a >&2; echo b"), "b\nError:\na");
    for invalid in [
        "invalid_command_name",
        "cd /nonexistent/path",
        "/bin/nonexistent",
    ] {
        let out = bash(invalid);
        assert!(out.starts_with("Error:\n"), "{invalid}: {out}");
    }
}

#[test]
fn background_jobs_and_comments_do_not_block_the_shell() {
    let start = Instant::now();
    assert_eq!(bash("sleep 5 &"), "");
    assert_eq!(bash("echo hi # trailing comment"), "hi");
    assert!(start.elapsed() < Duration::from_secs(4));
}

#[test]
fn a_shell_that_exits_early_is_reported() {
    assert_eq!(bash("exit 3"), "Error: Bash has exited with returncode 3");
}

#[test]
fn a_command_that_outlives_the_timeout_is_stopped() {
    let tools = Tools {
        bash: Bash {
            timeout: Duration::from_millis(300),
            ..Bash::default()
        },
    };
    let out = tools.call("bash", &json!({"command": "sleep 10"}));
    assert_eq!(
        out,
        "Error: Timed out: bash has not returned in 0.3 seconds and must be restarted."
    );
}

#[test]
fn a_missing_shell_is_an_error() {
    let missing = Bash {
        program: PathBuf::from("/no/such/bash"),
        ..Bash::default()
    };
    assert!(missing.call("echo hi").starts_with("Error: "));
    let silent = Bash {
        program: PathBuf::from("true"),
        ..Bash::default()
    };
    let out = silent.call(&"x".repeat(1 << 20));
    assert!(out.starts_with("Error: "), "{out}");
}
