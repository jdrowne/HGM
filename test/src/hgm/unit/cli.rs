use clap::Parser;
use hgm::cli::Cli;
use hgm_core::config::OverrideValue::{Bool, Float, Int, Str};

fn run_args(args: &[&str]) -> Cli {
    let mut argv = vec!["hgm"];
    argv.extend(args);
    Cli::try_parse_from(argv).unwrap()
}

#[test]
fn hgm_takes_no_sub_commands_or_positional_arguments() {
    let rejected: Vec<bool> = [
        &["hgm", "run"][..],
        &["hgm", "best"],
        &["hgm", "evaluate"],
        &["hgm", "prepare-polyglot"],
    ]
    .into_iter()
    .map(|argv| Cli::try_parse_from(argv).is_err())
    .collect();
    assert_eq!(rejected, [true; 4]);
}

#[test]
fn the_config_file_defaults_to_config_yaml() {
    assert_eq!(run_args(&[]).config.to_str(), Some("config.yaml"));
}

#[test]
fn defaults_override_only_the_initial_agent_name() {
    let args = run_args(&[]);
    let set: Vec<_> = args
        .overrides()
        .into_iter()
        .filter(|(_, v)| v.is_some())
        .collect();
    assert_eq!(
        set,
        [(
            "paths.initial_agent_name",
            Some(Str("default_agent".into()))
        )]
    );
}

#[test]
fn every_flag_becomes_an_override() {
    let args = run_args(&[
        "--config",
        "c.yaml",
        "--max_task_evals",
        "7",
        "--max_workers",
        "3",
        "--continue_from",
        "prev",
        "--output_dir",
        "out",
        "--polyglot",
        "--self_improve_llm",
        "a",
        "--downstream_llm",
        "b",
        "--diagnose_llm",
        "c",
        "--alpha",
        "0.5",
        "--cool_down",
        "--beta",
        "2",
        "--full_eval",
        "--self_improve_timeout",
        "11",
        "--evaluation_timeout",
        "12",
        "--n_pseudo_descendant_evals",
        "13",
        "--expansion_statistic",
        "own",
        "--eval_random_level",
        "0.25",
        "--initial_agent_name",
        "agent",
    ]);
    let overrides: Vec<_> = args
        .overrides()
        .into_iter()
        .map(|(k, v)| (k, v.unwrap()))
        .collect();
    assert_eq!(
        overrides,
        [
            ("execution.max_task_evals", Int(7)),
            ("execution.max_workers", Int(3)),
            ("paths.continue_from", Str("prev".into())),
            ("paths.output_dir", Str("out".into())),
            ("llm.self_improve_llm", Str("a".into())),
            ("llm.downstream_llm", Str("b".into())),
            ("llm.diagnose_llm", Str("c".into())),
            ("optimization.alpha", Float(0.5)),
            ("optimization.cool_down", Bool(true)),
            ("optimization.beta", Float(2.0)),
            ("evaluation.full_eval", Bool(true)),
            ("execution.self_improve_timeout", Int(11)),
            ("execution.evaluation_timeout", Int(12)),
            ("optimization.n_pseudo_descendant_evals", Int(13)),
            ("optimization.expansion_statistic", Str("own".into())),
            ("optimization.eval_random_level", Float(0.25)),
            ("evaluation.polyglot", Bool(true)),
            ("paths.initial_agent_name", Str("agent".into())),
        ]
    );
}

fn value(args: &[&str], key: &str) -> Option<hgm_core::config::OverrideValue> {
    run_args(args)
        .overrides()
        .into_iter()
        .find(|(k, _)| *k == key)
        .unwrap()
        .1
}

#[test]
fn a_negation_after_the_flag_wins() {
    assert_eq!(
        value(&["--polyglot", "--no_polyglot"], "evaluation.polyglot"),
        Some(Bool(false))
    );
}

#[test]
fn a_flag_after_its_negation_wins() {
    assert_eq!(
        value(&["--no_polyglot", "--polyglot"], "evaluation.polyglot"),
        Some(Bool(true))
    );
}

#[test]
fn a_lone_negation_sets_the_override_false() {
    assert_eq!(
        value(&["--no_cool_down"], "optimization.cool_down"),
        Some(Bool(false))
    );
}

#[test]
fn the_epsilon_defaults_to_one_percent() {
    assert_eq!(run_args(&[]).epsilon, 0.01);
}

#[test]
fn the_holdout_split_defaults_to_none() {
    assert_eq!(run_args(&[]).holdout_split, None);
}

#[test]
fn the_holdout_task_count_defaults_to_none() {
    assert_eq!(run_args(&[]).holdout_tasks, None);
}

fn pick_and_holdout_args() -> Cli {
    run_args(&[
        "--epsilon",
        "0.05",
        "--holdout_split",
        "Lite",
        "--holdout_tasks",
        "3",
    ])
}

#[test]
fn the_epsilon_flag_sets_the_epsilon() {
    assert_eq!(pick_and_holdout_args().epsilon, 0.05);
}

#[test]
fn the_holdout_split_flag_sets_the_split() {
    assert_eq!(
        pick_and_holdout_args().holdout_split.as_deref(),
        Some("Lite")
    );
}

#[test]
fn the_holdout_tasks_flag_sets_the_task_count() {
    assert_eq!(pick_and_holdout_args().holdout_tasks, Some(3));
}

#[test]
fn the_pick_and_holdout_flags_are_not_config_overrides() {
    assert_eq!(
        pick_and_holdout_args()
            .overrides()
            .iter()
            .filter(|(_, v)| v.is_some())
            .count(),
        1
    );
}

#[test]
fn the_default_cli_has_a_one_percent_epsilon() {
    assert_eq!(Cli::default().epsilon, 0.01);
}

#[test]
fn the_default_cli_names_the_default_agent() {
    assert_eq!(Cli::default().initial_agent_name, "default_agent");
}
