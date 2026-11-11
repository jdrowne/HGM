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

#[test]
fn flags_default_like_the_python() {
    let swe = args(&[]);
    assert_eq!(swe.outdir, "/hgm/");
    assert_eq!(swe.timeout, 3600);
    assert_eq!(swe.language, Language::Python);
    assert!(!swe.self_improve && !swe.polyglot);
    assert_eq!(swe.instance_id, None);
    assert_eq!(swe.model(), "gpt-5");
    assert_eq!(
        swe.patch_file(),
        std::path::Path::new("/hgm/model_patch.diff")
    );
    let polyglot = args(&[
        "--polyglot",
        "--language",
        "javascript",
        "--outdir",
        "",
        "--self_improve",
        "--instance_id",
        "i",
    ]);
    assert_eq!(polyglot.model(), "anthropic/claude-sonnet-4");
    assert_eq!(polyglot.language, Language::Javascript);
    assert_eq!(
        polyglot.patch_file(),
        std::path::Path::new("model_patch.diff")
    );
    assert_eq!(args(&["--model", "o3"]).model(), "o3");
    assert!(Args::try_parse_from(["hgm-agent", "--language", "cobol"]).is_err());
}

#[test]
fn instructions_match_the_python() {
    assert_eq!(
        args(&[]).instruction(),
        "I have uploaded a Python code repository in the directory /repo. Help solve the following problem.\n\n<problem_description>\np\n</problem_description>\n\n<test_description>\nNone\n</test_description>\n\nYour task is to make changes to the files in the /repo directory to address the <problem_description>. I have already taken care of the required dependencies.\n"
    );
    assert!(
        args(&["--test_description", "t"])
            .instruction()
            .contains("<test_description>\nt\n</test_description>")
    );
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
    assert!(ChatLog::create(tmp.path()).is_err());
}
