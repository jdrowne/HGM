use clap::{ArgAction, Parser};
use hgm_core::config::{
    EvaluationConfig, ExecutionConfig, ExpansionStatistic, HgmConfig, LlmConfig,
    OptimizationConfig, PathConfig,
};

#[derive(Clone, Debug, Parser)]
#[command(name = "hgm", version, about = "Huxley-Godel Machine")]
pub struct Cli {
    #[arg(long = "self_improve_llm", value_name = "MODEL")]
    pub self_improve_llm: String,
    #[arg(long = "downstream_llm", value_name = "MODEL")]
    pub downstream_llm: String,
    #[arg(long = "diagnose_llm", value_name = "MODEL")]
    pub diagnose_llm: String,
    #[arg(long, value_name = "X")]
    pub alpha: f64,
    #[arg(long, value_name = "X")]
    pub beta: f64,
    #[arg(long = "cool_down", action = ArgAction::Set, required = true, value_name = "BOOL")]
    pub cool_down: bool,
    #[arg(long = "eval_random_level", value_name = "P")]
    pub eval_random_level: f64,
    #[arg(long = "n_pseudo_descendant_evals", value_name = "N")]
    pub n_pseudo_descendant_evals: usize,
    #[arg(long = "expansion_statistic", value_name = "clade|own", value_parser = ExpansionStatistic::parse)]
    pub expansion_statistic: ExpansionStatistic,
    #[arg(long, value_name = "P")]
    pub epsilon: f64,
    #[arg(long = "max_workers", value_name = "N")]
    pub max_workers: usize,
    #[arg(long = "max_task_evals", value_name = "N")]
    pub max_task_evals: usize,
    #[arg(long = "self_improve_timeout", value_name = "SECS")]
    pub self_improve_timeout: u64,
    #[arg(long = "evaluation_timeout", value_name = "SECS")]
    pub evaluation_timeout: u64,
    #[arg(long = "full_eval", action = ArgAction::Set, required = true, value_name = "BOOL")]
    pub full_eval: bool,
    #[arg(long, action = ArgAction::Set, required = true, value_name = "BOOL")]
    pub polyglot: bool,
    #[arg(long = "output_dir", value_name = "DIR")]
    pub output_dir: String,
    #[arg(long = "initial_agent_name", value_name = "NAME")]
    pub initial_agent_name: String,
    #[arg(long = "continue_from", value_name = "DIR")]
    pub continue_from: Option<String>,
    #[arg(
        long = "holdout_split",
        value_name = "NAME",
        requires = "holdout_tasks"
    )]
    pub holdout_split: Option<String>,
    #[arg(long = "holdout_tasks", value_name = "N", requires = "holdout_split")]
    pub holdout_tasks: Option<usize>,
}

impl Cli {
    pub fn config(&self) -> HgmConfig {
        HgmConfig {
            llm: LlmConfig {
                self_improve_llm: self.self_improve_llm.clone(),
                downstream_llm: self.downstream_llm.clone(),
                diagnose_llm: self.diagnose_llm.clone(),
            },
            optimization: OptimizationConfig {
                alpha: self.alpha,
                beta: self.beta,
                cool_down: self.cool_down,
                eval_random_level: self.eval_random_level,
                n_pseudo_descendant_evals: self.n_pseudo_descendant_evals,
                expansion_statistic: self.expansion_statistic,
            },
            execution: ExecutionConfig {
                max_workers: self.max_workers,
                self_improve_timeout: self.self_improve_timeout,
                evaluation_timeout: self.evaluation_timeout,
                max_task_evals: self.max_task_evals,
            },
            evaluation: EvaluationConfig {
                full_eval: self.full_eval,
                polyglot: self.polyglot,
            },
            paths: PathConfig {
                output_dir: self.output_dir.clone(),
                continue_from: self.continue_from.clone(),
                initial_agent_name: self.initial_agent_name.clone(),
            },
        }
    }
}
