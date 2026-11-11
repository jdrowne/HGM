use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LlmConfig {
    pub self_improve_llm: String,
    pub downstream_llm: String,
    pub diagnose_llm: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpansionStatistic {
    Clade,
    Own,
}

impl ExpansionStatistic {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "clade" => Ok(Self::Clade),
            "own" => Ok(Self::Own),
            _ => Err(format!("expected clade or own, not {name}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptimizationConfig {
    pub alpha: f64,
    pub beta: f64,
    pub cool_down: bool,
    pub eval_random_level: f64,
    pub n_pseudo_descendant_evals: usize,
    pub expansion_statistic: ExpansionStatistic,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecutionConfig {
    pub max_workers: usize,
    pub self_improve_timeout: u64,
    pub evaluation_timeout: u64,
    pub max_task_evals: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvaluationConfig {
    pub full_eval: bool,
    pub polyglot: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathConfig {
    pub output_dir: String,
    pub continue_from: Option<String>,
    pub initial_agent_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HgmConfig {
    pub llm: LlmConfig,
    pub optimization: OptimizationConfig,
    pub execution: ExecutionConfig,
    pub evaluation: EvaluationConfig,
    pub paths: PathConfig,
}

impl HgmConfig {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("config serializes")
    }
}
