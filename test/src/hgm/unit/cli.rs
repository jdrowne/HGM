use clap::Parser;
use hgm::cli::{Cli, Command, RunArgs};
use hgm_core::config::OverrideValue::{Bool, Float, Int, Str};

fn run_args(args: &[&str]) -> RunArgs {
    let mut argv = vec!["hgm", "run"];
    argv.extend(args);
    let Command::Run(args) = Cli::try_parse_from(argv).unwrap().command else {
        panic!("not a run")
    };
    args
}

#[test]
fn prepare_polyglot_takes_no_arguments() {
    let cli = Cli::try_parse_from(["hgm", "prepare-polyglot"]).unwrap();
    assert!(matches!(cli.command, Command::PreparePolyglot));
    assert!(Cli::try_parse_from(["hgm", "prepare-polyglot", "x"]).is_err());
}

#[test]
fn defaults_override_only_the_initial_agent_name() {
    let args = run_args(&[]);
    assert_eq!(args.config.to_str(), Some("config.yaml"));
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
            ("optimization.eval_random_level", Float(0.25)),
            ("evaluation.polyglot", Bool(true)),
            ("paths.initial_agent_name", Str("agent".into())),
        ]
    );
}

#[test]
fn the_last_of_a_flag_and_its_negation_wins() {
    let value = |args: &[&str], key: &str| {
        run_args(args)
            .overrides()
            .into_iter()
            .find(|(k, _)| *k == key)
            .unwrap()
            .1
    };
    assert_eq!(
        value(&["--polyglot", "--no_polyglot"], "evaluation.polyglot"),
        Some(Bool(false))
    );
    assert_eq!(
        value(&["--no_polyglot", "--polyglot"], "evaluation.polyglot"),
        Some(Bool(true))
    );
    assert_eq!(
        value(&["--no_cool_down"], "optimization.cool_down"),
        Some(Bool(false))
    );
}

#[test]
fn evaluate_has_the_python_defaults() {
    let cli = Cli::try_parse_from(["hgm", "evaluate", "--agent_path", "a/b"]).unwrap();
    let Command::Evaluate(args) = cli.command else {
        panic!("not an evaluate");
    };
    assert_eq!(args.agent_path.to_str(), Some("a/b"));
    assert_eq!(args.results_dir.to_str(), Some("results"));
    assert_eq!(args.split, "Lite");
    assert_eq!(args.llm, "gpt-5");
    assert_eq!(args.hours_per_task, 5);
    assert_eq!(args.num_workers, 10);
    assert_eq!(args.n_tasks, None);
    let Command::Run(_) = Cli::try_parse_from(["hgm", "run"]).unwrap().command else {
        panic!("not a run");
    };
    assert!(Cli::try_parse_from(["hgm", "evaluate"]).is_err());
}
