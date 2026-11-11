use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LlmConfig {
    pub self_improve_llm: String,
    pub downstream_llm: String,
    pub diagnose_llm: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            self_improve_llm: "gpt-5-mini".into(),
            downstream_llm: "gpt-5-mini".into(),
            diagnose_llm: "gpt-5-mini".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpansionStatistic {
    #[default]
    Clade,
    Own,
}

impl ExpansionStatistic {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "clade" => Some(Self::Clade),
            "own" => Some(Self::Own),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OptimizationConfig {
    pub alpha: f64,
    pub beta: f64,
    pub cool_down: bool,
    pub eval_random_level: f64,
    pub n_pseudo_descendant_evals: usize,
    pub expansion_statistic: ExpansionStatistic,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            alpha: 0.6,
            beta: 1.0,
            cool_down: false,
            eval_random_level: 1.0,
            n_pseudo_descendant_evals: 10000,
            expansion_statistic: ExpansionStatistic::Clade,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExecutionConfig {
    pub max_workers: usize,
    pub self_improve_timeout: u64,
    pub evaluation_timeout: u64,
    pub max_task_evals: usize,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_workers: 16,
            self_improve_timeout: 3600,
            evaluation_timeout: 3600,
            max_task_evals: 800,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvaluationConfig {
    pub full_eval: bool,
    pub polyglot: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PathConfig {
    pub output_dir: Option<String>,
    pub continue_from: Option<String>,
    pub initial_agent_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HgmConfig {
    pub llm: LlmConfig,
    pub optimization: OptimizationConfig,
    pub execution: ExecutionConfig,
    pub evaluation: EvaluationConfig,
    pub paths: PathConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OverrideValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl HgmConfig {
    pub fn parse_yaml(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let value: serde_norway::Value = serde_norway::from_str(text).map_err(|e| e.to_string())?;
        if value.is_null() {
            return Ok(Self::default());
        }
        serde_norway::from_value(value).map_err(|e| e.to_string())
    }

    pub fn from_yaml(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => {
                eprintln!(
                    "Warning: Configuration file {} not found. Using defaults.",
                    path.display()
                );
                return Self::default();
            }
        };
        Self::from_yaml_str(&text, &path.display().to_string())
    }

    pub fn from_yaml_str(text: &str, origin: &str) -> Self {
        match Self::parse_yaml(text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error loading configuration from {origin}: {e}");
                eprintln!("Using default configuration.");
                Self::default()
            }
        }
    }

    pub fn load(path: &Path, overrides: &[(&str, Option<OverrideValue>)]) -> Self {
        let mut c = Self::from_yaml(path);
        for (key, value) in overrides {
            if let Some(v) = value {
                c.set(key, v.clone());
            }
        }
        c
    }

    pub fn set(&mut self, path: &str, value: OverrideValue) -> bool {
        use OverrideValue as V;
        let Some((section, field)) = path.split_once('.') else {
            return false;
        };
        let s = |v: &V| match v {
            V::Str(s) => Some(s.clone()),
            _ => None,
        };
        let f = |v: &V| match v {
            V::Float(f) => Some(*f),
            V::Int(i) => Some(*i as f64),
            _ => None,
        };
        let u = |v: &V| match v {
            V::Int(i) if *i >= 0 => Some(*i as u64),
            _ => None,
        };
        let b = |v: &V| match v {
            V::Bool(b) => Some(*b),
            _ => None,
        };
        fn put<T>(slot: &mut T, v: Option<T>) -> bool {
            v.map(|v| *slot = v).is_some()
        }
        match (section, field) {
            ("llm", "self_improve_llm") => put(&mut self.llm.self_improve_llm, s(&value)),
            ("llm", "downstream_llm") => put(&mut self.llm.downstream_llm, s(&value)),
            ("llm", "diagnose_llm") => put(&mut self.llm.diagnose_llm, s(&value)),
            ("optimization", "alpha") => put(&mut self.optimization.alpha, f(&value)),
            ("optimization", "beta") => put(&mut self.optimization.beta, f(&value)),
            ("optimization", "cool_down") => put(&mut self.optimization.cool_down, b(&value)),
            ("optimization", "eval_random_level") => {
                put(&mut self.optimization.eval_random_level, f(&value))
            }
            ("optimization", "n_pseudo_descendant_evals") => put(
                &mut self.optimization.n_pseudo_descendant_evals,
                u(&value).map(|x| x as usize),
            ),
            ("optimization", "expansion_statistic") => put(
                &mut self.optimization.expansion_statistic,
                s(&value).as_deref().and_then(ExpansionStatistic::parse),
            ),
            ("execution", "max_workers") => put(
                &mut self.execution.max_workers,
                u(&value).map(|x| x as usize),
            ),
            ("execution", "self_improve_timeout") => {
                put(&mut self.execution.self_improve_timeout, u(&value))
            }
            ("execution", "evaluation_timeout") => {
                put(&mut self.execution.evaluation_timeout, u(&value))
            }
            ("execution", "max_task_evals") => put(
                &mut self.execution.max_task_evals,
                u(&value).map(|x| x as usize),
            ),
            ("evaluation", "full_eval") => put(&mut self.evaluation.full_eval, b(&value)),
            ("evaluation", "polyglot") => put(&mut self.evaluation.polyglot, b(&value)),
            ("paths", "output_dir") => put(&mut self.paths.output_dir, s(&value).map(Some)),
            ("paths", "continue_from") => put(&mut self.paths.continue_from, s(&value).map(Some)),
            ("paths", "initial_agent_name") => put(&mut self.paths.initial_agent_name, s(&value)),
            _ => false,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("config serializes")
    }

    pub fn to_yaml(&self) -> String {
        fn scalar(v: &serde_json::Value) -> String {
            match v {
                serde_json::Value::Number(n) if n.as_i64().is_none() => {
                    crate::pyjson::float_repr(n.as_f64().unwrap_or(f64::NAN))
                }
                serde_json::Value::String(s) if s.is_empty() => "''".into(),
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }
        }
        let mut out = String::new();
        let root = self.to_json();
        let mut sections: Vec<_> = root.as_object().expect("object").iter().collect();
        sections.sort_by_key(|(k, _)| k.as_str());
        for (name, fields) in sections {
            out.push_str(name);
            out.push_str(":\n");
            let mut fields: Vec<_> = fields.as_object().expect("object").iter().collect();
            fields.sort_by_key(|(k, _)| k.as_str());
            for (k, v) in fields {
                out.push_str(&format!("  {k}: {}\n", scalar(v)));
            }
        }
        out
    }
}
