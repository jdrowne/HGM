use std::path::Path;

use clap::Parser;
use hgm_agent::agent::{Args, ChatLog, Language};

fn args(extra: &[&str]) -> Args {
    let base = [
        "hgm-agent",
        "--problem_statement",
        "p",
        "--git_dir",
        "/repo",
        "--base_commit",
        "c",
        "--chat_history_file",
        "chat.md",
    ];
    Args::try_parse_from(base.iter().chain(extra)).unwrap()
}

fn polyglot_args() -> Args {
    args(&[
        "--polyglot",
        "--language",
        "javascript",
        "--outdir",
        "",
        "--self_improve",
        "--instance_id",
        "i",
    ])
}

#[test]
fn the_outdir_defaults_to_hgm() {
    assert_eq!(args(&[]).outdir, "/hgm/");
}

#[test]
fn the_timeout_defaults_to_an_hour() {
    assert_eq!(args(&[]).timeout, 3600);
}

#[test]
fn the_language_defaults_to_python() {
    assert_eq!(args(&[]).language, Language::Python);
}

#[test]
fn self_improve_defaults_off() {
    assert!(!args(&[]).self_improve);
}

#[test]
fn polyglot_defaults_off() {
    assert!(!args(&[]).polyglot);
}

#[test]
fn the_instance_id_defaults_to_none() {
    assert_eq!(args(&[]).instance_id, None);
}

#[test]
fn the_swe_model_defaults_to_gpt_5() {
    assert_eq!(args(&[]).model(), "gpt-5");
}

#[test]
fn the_swe_patch_file_lives_in_the_outdir() {
    assert_eq!(args(&[]).patch_file(), Path::new("/hgm/model_patch.diff"));
}

#[test]
fn the_polyglot_model_defaults_to_claude_sonnet() {
    assert_eq!(polyglot_args().model(), "anthropic/claude-sonnet-4");
}

#[test]
fn the_language_flag_is_parsed() {
    assert_eq!(polyglot_args().language, Language::Javascript);
}

#[test]
fn an_empty_outdir_puts_the_patch_file_in_the_working_directory() {
    assert_eq!(polyglot_args().patch_file(), Path::new("model_patch.diff"));
}

#[test]
fn an_explicit_model_wins() {
    assert_eq!(args(&["--model", "o3"]).model(), "o3");
}

#[test]
fn an_unknown_language_is_rejected() {
    assert!(Args::try_parse_from(["hgm-agent", "--language", "cobol"]).is_err());
}

#[test]
fn the_swe_instruction_matches_the_python() {
    assert_eq!(
        args(&[]).instruction(),
        "I have uploaded a Python code repository in the directory /repo. Help solve the following problem.\n\n<problem_description>\np\n</problem_description>\n\n<test_description>\nNone\n</test_description>\n\nYour task is to make changes to the files in the /repo directory to address the <problem_description>. I have already taken care of the required dependencies.\n"
    );
}

#[test]
fn the_instruction_carries_the_test_description() {
    assert!(
        args(&["--test_description", "t"])
            .instruction()
            .contains("<test_description>\nt\n</test_description>")
    );
}

#[test]
fn the_polyglot_instruction_matches_the_python() {
    assert_eq!(
        args(&["--polyglot"]).instruction(),
        "I have uploaded a code repository in the directory /repo. Help solve the following problem.\n\n<problem_description>\np\n</problem_description>\n\nYour task is to make changes to the files in the /repo directory to address the <problem_description>. I have already taken care of the required dependencies.\n\n\nPlease analyze the problem description carefully. Then make edits to the code files to complete the instruction."
    );
}

#[test]
fn the_chat_log_starts_empty_and_appends_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("a/b/chat.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "old").unwrap();
    let log = ChatLog::create(&path).unwrap();
    log.log("one");
    log.log("two");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
}

#[test]
fn a_chat_log_cannot_be_created_over_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(ChatLog::create(tmp.path()).is_err());
}
