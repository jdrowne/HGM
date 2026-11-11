use std::path::{Path, PathBuf};

use clap::{ArgAction, Parser};
use rustyhgm_bench::dataset::Hub;
use rustyhgm_bench::swebench::Swebench;
use rustyhgm_coding_agent::llm::{Provider, Service, read_api_key};
use rustyhgm_core::config::{
    EvaluationConfig, ExecutionConfig, ExpansionStatistic, HgmConfig, LlmConfig,
    OptimizationConfig, PathConfig,
};
use rustyhgm_sandbox::Docker;

#[derive(Clone, Debug, Parser)]
#[command(name = "rustyhgm", version, about = "Huxley-Godel Machine")]
pub struct Cli {
    #[arg(long = "self_improve_llm", value_name = "MODEL")]
    pub self_improve_llm: String,
    #[arg(long = "self_improve_provider", value_enum)]
    pub self_improve_provider: Provider,
    #[arg(long = "self_improve_base_url", value_name = "URL")]
    pub self_improve_base_url: String,
    #[arg(long = "downstream_llm", value_name = "MODEL")]
    pub downstream_llm: String,
    #[arg(long = "downstream_provider", value_enum)]
    pub downstream_provider: Provider,
    #[arg(long = "downstream_base_url", value_name = "URL")]
    pub downstream_base_url: String,
    #[arg(long = "diagnose_llm", value_name = "MODEL")]
    pub diagnose_llm: String,
    #[arg(long = "diagnose_provider", value_enum)]
    pub diagnose_provider: Provider,
    #[arg(long = "diagnose_base_url", value_name = "URL")]
    pub diagnose_base_url: String,
    #[arg(long = "api_key_file", value_name = "FILE")]
    pub api_key_file: PathBuf,
    #[arg(long, value_name = "PROGRAM")]
    pub docker: String,
    #[arg(long = "swebench_python", value_name = "PROGRAM")]
    pub swebench_python: String,
    #[arg(long = "dataset_endpoint", value_name = "URL")]
    pub dataset_endpoint: String,
    #[arg(long = "dataset_dir", value_name = "DIR")]
    pub dataset_dir: PathBuf,
    #[arg(long = "polyglot_repo", value_name = "URL")]
    pub polyglot_repo: String,
    #[arg(long = "polyglot_dir", value_name = "DIR")]
    pub polyglot_dir: PathBuf,
    #[arg(long = "progenitor_dir", value_name = "DIR")]
    pub progenitor_dir: PathBuf,
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
    #[arg(long = "progenitor_name", value_name = "NAME")]
    pub progenitor_name: String,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Services {
    pub self_improve: Service,
    pub downstream: Service,
    pub diagnose: Service,
    pub api_key: String,
}

#[derive(Clone, Debug)]
pub struct Tools {
    pub docker: Docker,
    pub swebench: Swebench,
    pub hub: Hub,
}

impl Cli {
    pub fn tools(&self, root_dir: &Path) -> Tools {
        Tools {
            docker: Docker::new(&self.docker),
            swebench: Swebench {
                python: self.swebench_python.clone(),
            },
            hub: Hub {
                endpoint: self.dataset_endpoint.clone(),
                cache_dir: root_dir.join(&self.dataset_dir),
            },
        }
    }

    pub fn services(&self, root_dir: &Path) -> Result<Services, String> {
        let api_key_file = root_dir.join(&self.api_key_file);
        let api_key = read_api_key(&api_key_file)?;
        let service = |provider: Provider, base_url: &str| Service {
            provider,
            base_url: base_url.to_owned(),
            api_key_file: api_key_file.clone(),
        };
        Ok(Services {
            self_improve: service(self.self_improve_provider, &self.self_improve_base_url),
            downstream: service(self.downstream_provider, &self.downstream_base_url),
            diagnose: service(self.diagnose_provider, &self.diagnose_base_url),
            api_key,
        })
    }

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
                progenitor_name: self.progenitor_name.clone(),
            },
        }
    }
}
