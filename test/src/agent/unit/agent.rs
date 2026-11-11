use std::path::Path;

use clap::Parser;
use hgm_agent::agent::{Args, ChatLog, Language};
use hgm_agent::llm::{Provider, Service};

const FLAGS: [(&str, &str); 14] = [
    ("--problem_statement", "p"),
    ("--git_dir", "/repo"),
    ("--base_commit", "c"),
    ("--chat_history_file", "chat.md"),
    ("--outdir", "/hgm/"),
    ("--test_description", "None"),
    ("--self_improve", "false"),
    ("--polyglot", "false"),
    ("--language", "python"),
    ("--model", "m"),
    ("--provider", "vllm"),
    ("--base_url", "http://h/v1"),
    ("--api_key_file", "/key"),
    ("--timeout", "3600"),
];

fn argv(changes: &[(&str, &str)], without: &str) -> Vec<String> {
    let mut argv = vec!["hgm-agent".to_owned()];
    for (flag, value) in FLAGS.iter().filter(|(flag, _)| *flag != without) {
        let changed = changes.iter().find(|(f, _)| f == flag);
        argv.push((*flag).to_owned());
        argv.push(changed.map_or(*value, |(_, v)| *v).to_owned());
    }
    argv
}

fn args(changes: &[(&str, &str)]) -> Args {
    Args::try_parse_from(argv(changes, "")).unwrap()
}

#[test]
fn every_flag_but_the_instance_id_is_required_and_named_when_missing() {
    let unnamed: Vec<&str> = FLAGS
        .iter()
        .map(|(flag, _)| *flag)
        .filter(|flag| {
            !Args::try_parse_from(argv(&[], flag))
                .unwrap_err()
                .to_string()
                .contains(flag)
        })
        .collect();
    assert_eq!(unnamed, Vec::<&str>::new());
}

#[test]
fn the_instance_id_is_optional() {
    assert_eq!(args(&[]).instance_id, None);
}

#[test]
fn the_service_comes_from_its_flags() {
    assert_eq!(
        args(&[("--provider", "openai")]).service(),
        Service {
            provider: Provider::OpenAi,
            base_url: "http://h/v1".into(),
            api_key_file: "/key".into(),
        }
    );
}

#[test]
fn an_unknown_provider_is_rejected() {
    assert!(Args::try_parse_from(argv(&[("--provider", "anthropic")], "")).is_err());
}

#[test]
fn a_yes_no_flag_takes_only_true_or_false() {
    assert!(Args::try_parse_from(argv(&[("--polyglot", "yes")], "")).is_err());
}

#[test]
fn the_swe_patch_file_lives_in_the_outdir() {
    assert_eq!(args(&[]).patch_file(), Path::new("/hgm/model_patch.diff"));
}

#[test]
fn the_language_flag_is_parsed() {
    assert_eq!(
        args(&[("--language", "javascript")]).language,
        Language::Javascript
    );
}

#[test]
fn an_empty_outdir_puts_the_patch_file_in_the_working_directory() {
    assert_eq!(
        args(&[("--outdir", "")]).patch_file(),
        Path::new("model_patch.diff")
    );
}

#[test]
fn an_unknown_language_is_rejected() {
    assert!(Args::try_parse_from(argv(&[("--language", "cobol")], "")).is_err());
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
        args(&[("--test_description", "t")])
            .instruction()
            .contains("<test_description>\nt\n</test_description>")
    );
}

#[test]
fn the_polyglot_instruction_matches_the_python() {
    assert_eq!(
        args(&[("--polyglot", "true")]).instruction(),
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
