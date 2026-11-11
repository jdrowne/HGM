use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};
use hgm_core::config::OverrideValue;

#[derive(Debug, Parser)]
#[command(name = "hgm", version, about = "Huxley-Godel Machine")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run(RunArgs),
    Evaluate(EvaluateArgs),
    PreparePolyglot,
    Best(BestArgs),
}

#[derive(Clone, Debug, Default, Args)]
pub struct RunArgs {
    #[arg(long, default_value = "config.yaml")]
    pub config: PathBuf,
    #[arg(long = "max_task_evals")]
    pub max_task_evals: Option<i64>,
    #[arg(long = "max_workers")]
    pub max_workers: Option<i64>,
    #[arg(long = "continue_from")]
    pub continue_from: Option<String>,
    #[arg(long = "output_dir")]
    pub output_dir: Option<String>,
    #[arg(long, action = ArgAction::SetTrue, overrides_with = "no_polyglot")]
    pub polyglot: bool,
    #[arg(long = "no_polyglot", action = ArgAction::SetTrue, overrides_with = "polyglot")]
    pub no_polyglot: bool,
    #[arg(long = "self_improve_llm")]
    pub self_improve_llm: Option<String>,
    #[arg(long = "downstream_llm")]
    pub downstream_llm: Option<String>,
    #[arg(long = "diagnose_llm")]
    pub diagnose_llm: Option<String>,
    #[arg(long)]
    pub alpha: Option<f64>,
    #[arg(long = "cool_down", action = ArgAction::SetTrue, overrides_with = "no_cool_down")]
    pub cool_down: bool,
    #[arg(long = "no_cool_down", action = ArgAction::SetTrue, overrides_with = "cool_down")]
    pub no_cool_down: bool,
    #[arg(long)]
    pub beta: Option<f64>,
    #[arg(long = "full_eval", action = ArgAction::SetTrue)]
    pub full_eval: bool,
    #[arg(long = "self_improve_timeout")]
    pub self_improve_timeout: Option<i64>,
    #[arg(long = "evaluation_timeout")]
    pub evaluation_timeout: Option<i64>,
    #[arg(long = "n_pseudo_descendant_evals")]
    pub n_pseudo_descendant_evals: Option<i64>,
    #[arg(long = "eval_random_level")]
    pub eval_random_level: Option<f64>,
    #[arg(long = "initial_agent_name", default_value = "default_agent")]
    pub initial_agent_name: String,
}

fn either(on: bool, off: bool) -> Option<bool> {
    if on {
        Some(true)
    } else if off {
        Some(false)
    } else {
        None
    }
}

impl RunArgs {
    pub fn overrides(&self) -> Vec<(&'static str, Option<OverrideValue>)> {
        use OverrideValue::{Bool, Float, Int, Str};
        vec![
            ("execution.max_task_evals", self.max_task_evals.map(Int)),
            ("execution.max_workers", self.max_workers.map(Int)),
            ("paths.continue_from", self.continue_from.clone().map(Str)),
            ("paths.output_dir", self.output_dir.clone().map(Str)),
            (
                "llm.self_improve_llm",
                self.self_improve_llm.clone().map(Str),
            ),
            ("llm.downstream_llm", self.downstream_llm.clone().map(Str)),
            ("llm.diagnose_llm", self.diagnose_llm.clone().map(Str)),
            ("optimization.alpha", self.alpha.map(Float)),
            (
                "optimization.cool_down",
                either(self.cool_down, self.no_cool_down).map(Bool),
            ),
            ("optimization.beta", self.beta.map(Float)),
            ("evaluation.full_eval", self.full_eval.then_some(Bool(true))),
            (
                "execution.self_improve_timeout",
                self.self_improve_timeout.map(Int),
            ),
            (
                "execution.evaluation_timeout",
                self.evaluation_timeout.map(Int),
            ),
            (
                "optimization.n_pseudo_descendant_evals",
                self.n_pseudo_descendant_evals.map(Int),
            ),
            (
                "optimization.eval_random_level",
                self.eval_random_level.map(Float),
            ),
            (
                "evaluation.polyglot",
                either(self.polyglot, self.no_polyglot).map(Bool),
            ),
            (
                "paths.initial_agent_name",
                Some(Str(self.initial_agent_name.clone())),
            ),
        ]
    }
}

#[derive(Clone, Debug, Args)]
pub struct EvaluateArgs {
    #[arg(long = "agent_path")]
    pub agent_path: PathBuf,
    #[arg(long = "results_dir", default_value = "results")]
    pub results_dir: PathBuf,
    #[arg(long, default_value = "Lite")]
    pub split: String,
    #[arg(long, default_value = "gpt-5")]
    pub llm: String,
    #[arg(long = "hours_per_task", default_value_t = 5)]
    pub hours_per_task: u64,
    #[arg(long = "num_workers", default_value_t = 10)]
    pub num_workers: usize,
    #[arg(long = "n_tasks")]
    pub n_tasks: Option<usize>,
}

#[derive(Clone, Debug, Args)]
pub struct BestArgs {
    #[arg(long = "run_dir")]
    pub run_dir: PathBuf,
    #[arg(long, default_value_t = 0.01)]
    pub epsilon: f64,
}
