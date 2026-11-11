use clap::Parser;
use hgm::cli::Cli;
use serde_json::json;

use crate::common::{SETTINGS, argv, cli};

fn parse_error(changes: &[(&str, &str)], without: &[&str]) -> String {
    Cli::try_parse_from(argv(changes, without))
        .unwrap_err()
        .to_string()
}

#[test]
fn hgm_takes_no_sub_commands_or_positional_arguments() {
    let rejected: Vec<bool> = ["run", "best", "evaluate", "prepare-polyglot"]
        .into_iter()
        .map(|word| {
            let mut args = argv(&[], &[]);
            args.push(word.to_owned());
            Cli::try_parse_from(args).is_err()
        })
        .collect();
    assert_eq!(rejected, [true; 4]);
}

#[test]
fn every_setting_is_required_and_named_when_missing() {
    let unnamed: Vec<&str> = SETTINGS
        .iter()
        .map(|(flag, _)| *flag)
        .filter(|flag| !parse_error(&[], &[flag]).contains(flag))
        .collect();
    assert_eq!(unnamed, Vec::<&str>::new());
}

#[test]
fn every_flag_reaches_the_configuration() {
    let args = cli(&[
        ("--self_improve_llm", "a"),
        ("--downstream_llm", "b"),
        ("--diagnose_llm", "c"),
        ("--alpha", "0.5"),
        ("--beta", "2"),
        ("--cool_down", "true"),
        ("--eval_random_level", "0.25"),
        ("--n_pseudo_descendant_evals", "13"),
        ("--expansion_statistic", "own"),
        ("--max_workers", "3"),
        ("--max_task_evals", "7"),
        ("--self_improve_timeout", "11"),
        ("--evaluation_timeout", "12"),
        ("--full_eval", "true"),
        ("--polyglot", "true"),
        ("--output_dir", "out"),
        ("--initial_agent_name", "agent"),
        ("--continue_from", "prev"),
    ]);
    assert_eq!(
        args.config().to_json(),
        json!({
            "llm": {"self_improve_llm": "a", "downstream_llm": "b", "diagnose_llm": "c"},
            "optimization": {
                "alpha": 0.5,
                "beta": 2.0,
                "cool_down": true,
                "eval_random_level": 0.25,
                "n_pseudo_descendant_evals": 13,
                "expansion_statistic": "own",
            },
            "execution": {
                "max_workers": 3,
                "self_improve_timeout": 11,
                "evaluation_timeout": 12,
                "max_task_evals": 7,
            },
            "evaluation": {"full_eval": true, "polyglot": true},
            "paths": {"output_dir": "out", "continue_from": "prev", "initial_agent_name": "agent"},
        })
    );
}

#[test]
fn a_yes_no_flag_needs_its_value() {
    let mut args = argv(&[], &["--cool_down"]);
    args.push("--cool_down".to_owned());
    assert!(Cli::try_parse_from(args).is_err());
}

#[test]
fn a_yes_no_flag_takes_only_true_or_false() {
    assert!(parse_error(&[("--polyglot", "yes")], &[]).contains("--polyglot"));
}

#[test]
fn an_unknown_expansion_statistic_is_refused() {
    assert!(parse_error(&[("--expansion_statistic", "both")], &[]).contains("clade or own"));
}

#[test]
fn a_negative_worker_count_is_refused() {
    assert!(parse_error(&[("--max_workers", "-1")], &[]).contains("--max_workers"));
}

#[test]
fn the_run_to_continue_from_is_optional() {
    assert_eq!(cli(&[]).continue_from, None);
}

#[test]
fn the_holdout_split_is_optional() {
    assert_eq!(cli(&[]).holdout_split, None);
}

#[test]
fn a_holdout_split_needs_a_task_count() {
    assert!(parse_error(&[("--holdout_split", "Lite")], &[]).contains("--holdout_tasks"));
}

#[test]
fn a_holdout_task_count_needs_a_split() {
    assert!(parse_error(&[("--holdout_tasks", "3")], &[]).contains("--holdout_split"));
}

#[test]
fn the_holdout_flags_set_the_split_and_task_count() {
    let args = cli(&[("--holdout_split", "Lite"), ("--holdout_tasks", "3")]);
    assert_eq!(
        (args.holdout_split.as_deref(), args.holdout_tasks),
        (Some("Lite"), Some(3))
    );
}

#[test]
fn the_epsilon_flag_sets_the_epsilon() {
    assert_eq!(cli(&[("--epsilon", "0.05")]).epsilon, 0.05);
}
