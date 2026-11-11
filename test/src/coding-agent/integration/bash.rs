use std::path::PathBuf;
use std::time::{Duration, Instant};

use rustyhgm_coding_agent::tools::{Bash, Tools};
use serde_json::json;

fn bash(command: &str) -> String {
    Tools::default().call("bash", &json!({"command": command}))
}

#[test]
fn a_simple_command_prints_its_output() {
    assert_eq!(bash("echo 'hello world'"), "hello world");
}

#[test]
fn chained_commands_print_each_output() {
    assert_eq!(bash("echo 'first' && echo 'second'"), "first\nsecond");
}

#[test]
fn a_longer_chain_prints_every_line() {
    assert_eq!(
        bash("echo 'line1' && echo 'line2' && echo 'line3'"),
        "line1\nline2\nline3"
    );
}

#[test]
fn printf_output_keeps_its_lines() {
    assert_eq!(
        bash("printf 'line1\\nline2\\nline3'"),
        "line1\nline2\nline3"
    );
}

#[test]
fn variables_carry_across_a_chain() {
    assert_eq!(bash("TEST_VAR='hello' && echo $TEST_VAR"), "hello");
}

#[test]
fn a_slow_command_is_waited_for() {
    assert_eq!(bash("sleep 1 && echo 'done'"), "done");
}

#[test]
fn special_characters_are_printed_as_is() {
    assert_eq!(
        bash("echo 'test with spaces and !@#$%^&*()'"),
        "test with spaces and !@#$%^&*()"
    );
}

fn hundred_lines() -> String {
    bash("for i in {1..100}; do echo \"Line $i\"; done")
}

#[test]
fn large_output_keeps_its_first_line() {
    assert!(hundred_lines().starts_with("Line 1\n"));
}

#[test]
fn large_output_keeps_its_last_line() {
    assert!(hundred_lines().ends_with("Line 100"));
}

#[test]
fn very_large_output_is_returned_whole() {
    assert_eq!(
        bash("head -c 300000 /dev/zero | tr '\\0' 'a'").len(),
        300000
    );
}

fn missing_directory() -> String {
    bash("ls /nonexistent/directory")
}

#[test]
fn an_error_is_labelled() {
    let out = missing_directory();
    assert!(out.starts_with("Error:\nls: "), "{out}");
}

#[test]
fn an_error_keeps_the_message() {
    let out = missing_directory();
    assert!(out.contains("No such file or directory"), "{out}");
}

#[test]
fn errors_follow_the_output() {
    assert_eq!(bash("echo a >&2; echo b"), "b\nError:\na");
}

#[test]
fn invalid_commands_are_errors() {
    let unlabelled: Vec<String> = [
        "invalid_command_name",
        "cd /nonexistent/path",
        "/bin/nonexistent",
    ]
    .into_iter()
    .map(bash)
    .filter(|out| !out.starts_with("Error:\n"))
    .collect();
    assert_eq!(unlabelled, Vec::<String>::new());
}

#[test]
fn a_background_job_prints_nothing() {
    assert_eq!(bash("sleep 5 &"), "");
}

#[test]
fn a_background_job_does_not_block_the_shell() {
    let start = Instant::now();
    bash("sleep 5 &");
    assert!(start.elapsed() < Duration::from_secs(4));
}

#[test]
fn a_trailing_comment_is_ignored() {
    assert_eq!(bash("echo hi # trailing comment"), "hi");
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
}

#[test]
fn a_shell_that_reads_nothing_is_an_error() {
    let silent = Bash {
        program: PathBuf::from("true"),
        ..Bash::default()
    };
    let out = silent.call(&"x".repeat(1 << 20));
    assert!(out.starts_with("Error: "), "{out}");
}
