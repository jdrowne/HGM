use rustyhgm_core::config::{
    EvaluationConfig, ExecutionConfig, ExpansionStatistic, HgmConfig, LlmConfig,
    OptimizationConfig, PathConfig,
};
use serde_json::json;

fn config() -> HgmConfig {
    HgmConfig {
        llm: LlmConfig {
            self_improve_llm: "a".into(),
            downstream_llm: "b".into(),
            diagnose_llm: "c".into(),
        },
        optimization: OptimizationConfig {
            alpha: 0.6,
            beta: 1.0,
            cool_down: false,
            eval_random_level: 1.0,
            n_pseudo_descendant_evals: 10000,
            expansion_statistic: ExpansionStatistic::Clade,
        },
        execution: ExecutionConfig {
            max_workers: 16,
            self_improve_timeout: 3600,
            evaluation_timeout: 3601,
            max_task_evals: 800,
        },
        evaluation: EvaluationConfig {
            full_eval: false,
            polyglot: true,
        },
        paths: PathConfig {
            output_dir: "out".into(),
            continue_from: None,
            progenitor_name: "harness".into(),
        },
    }
}

#[test]
fn the_configuration_is_logged_by_section() {
    assert_eq!(
        config().to_json(),
        json!({
            "llm": {"self_improve_llm": "a", "downstream_llm": "b", "diagnose_llm": "c"},
            "optimization": {
                "alpha": 0.6,
                "beta": 1.0,
                "cool_down": false,
                "eval_random_level": 1.0,
                "n_pseudo_descendant_evals": 10000,
                "expansion_statistic": "clade",
            },
            "execution": {
                "max_workers": 16,
                "self_improve_timeout": 3600,
                "evaluation_timeout": 3601,
                "max_task_evals": 800,
            },
            "evaluation": {"full_eval": false, "polyglot": true},
            "paths": {"output_dir": "out", "continue_from": null, "progenitor_name": "harness"},
        })
    );
}

#[test]
fn the_expansion_statistic_parses_own() {
    assert_eq!(
        ExpansionStatistic::parse("own"),
        Ok(ExpansionStatistic::Own)
    );
}

#[test]
fn the_expansion_statistic_parses_clade() {
    assert_eq!(
        ExpansionStatistic::parse("clade"),
        Ok(ExpansionStatistic::Clade)
    );
}

#[test]
fn an_unknown_expansion_statistic_names_the_choices() {
    assert_eq!(
        ExpansionStatistic::parse("both"),
        Err("expected clade or own, not both".to_owned())
    );
}
