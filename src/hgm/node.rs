use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};

use hgm_bench::polyglot;
use hgm_bench::report::Report;
use hgm_bench::swe::{self, Run};
use hgm_core::metadata::{self, NODE_METADATA_FILE, OverallPerformance};
use hgm_core::pyjson::{self, PyValue};

use crate::agent_build::{AGENT_BINARY, Build};

pub type Metadata = Vec<(String, PyValue)>;

pub fn at(path: &Path, e: impl Display) -> String {
    format!("{}: {e}", path.display())
}

pub fn put(metadata: &mut Metadata, key: &str, value: PyValue) {
    match metadata.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => metadata.push((key.to_owned(), value)),
    }
}

pub fn save_metadata(dir: &Path, metadata: &Metadata) -> Result<(), String> {
    let path = dir.join(NODE_METADATA_FILE);
    let text = pyjson::dumps(&PyValue::Dict(metadata.clone()), Some(4));
    fs::write(&path, text).map_err(|e| at(&path, e))
}

pub fn load_metadata(dir: &Path) -> Result<Metadata, String> {
    let path = dir.join(NODE_METADATA_FILE);
    fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| pyjson::parse_lenient(&text).map_err(|e| e.to_string()))
        .and_then(|value| match PyValue::from(&value) {
            PyValue::Dict(items) => Ok(items),
            _ => Err("not a JSON object".to_owned()),
        })
        .map_err(|e| at(&path, e))
}

pub fn empty_metadata(commit_id: &str) -> Metadata {
    vec![
        ("run_id".to_owned(), PyValue::str(commit_id)),
        (
            "overall_performance".to_owned(),
            OverallPerformance::default().to_py(),
        ),
    ]
}

pub struct Eval<'a> {
    pub commit_id: &'a str,
    pub tasks: &'a [String],
    pub max_workers: usize,
    pub agent: PathBuf,
    pub node_dir: &'a Path,
}

pub trait Bench: Sync {
    fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String>;
}

fn agent_run(eval: &Eval) -> Run {
    Run {
        model_name_or_path: eval.commit_id.to_owned(),
        agent: eval.agent.clone(),
        max_workers: eval.max_workers,
    }
}

impl Bench for swe::Harness {
    fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
        let dnames = self.harness(
            eval.tasks,
            &agent_run(eval),
            &eval.node_dir.join("predictions"),
        )?;
        println!("Start make_report");
        let run_ids: Vec<String> = (0..dnames.len())
            .map(|i| format!("{}_{i}", eval.commit_id))
            .collect();
        let report = Report {
            output_dir: eval.node_dir.to_path_buf(),
            num_eval_procs: eval.max_workers,
            ..Report::default()
        };
        report.make(&self.swebench, &dnames, Some(&run_ids));
        println!("Start get_performance");
        Ok(Some(dnames))
    }
}

impl Bench for polyglot::Harness {
    fn run(&self, eval: &Eval) -> Result<Option<Vec<PathBuf>>, String> {
        self.harness(
            eval.tasks,
            &agent_run(eval),
            &eval.node_dir.join("predictions"),
            eval.node_dir,
        )
        .map(|_| None)
    }
}

pub struct Evaluator<'a> {
    pub bench: &'a dyn Bench,
    pub output_dir: &'a Path,
    pub max_workers: usize,
    pub builder: &'a dyn Build,
}

impl Evaluator<'_> {
    pub fn eval_agent(&self, commit_id: &str, tasks: &[String]) -> Result<Vec<f64>, String> {
        let output_dir = self.output_dir;
        let node_dir = output_dir.join(commit_id);
        if !node_dir.join(NODE_METADATA_FILE).exists() {
            save_metadata(&node_dir, &empty_metadata(commit_id))?;
        }
        let mut metadata = load_metadata(&node_dir)?;
        let agent = node_dir.join(AGENT_BINARY);
        if !tasks.is_empty() && !agent.exists() {
            let patches = metadata::model_patch_paths(output_dir, "", commit_id)
                .map_err(|e| e.to_string())?;
            self.builder.build(&patches, &agent)?;
        }
        let dnames = self.bench.run(&Eval {
            commit_id,
            tasks,
            max_workers: self.max_workers.min(tasks.len()),
            agent,
            node_dir: &node_dir,
        })?;
        let (_, overall) =
            metadata::get_all_performance(&node_dir, commit_id).map_err(|e| at(&node_dir, e))?;
        if let Some(dnames) = dnames {
            println!("End of evaluation");
            let shown = dnames
                .iter()
                .map(|d| PyValue::str(d.display().to_string()))
                .collect();
            put(&mut metadata, "swe_dnames", PyValue::List(shown));
        }
        put(&mut metadata, "overall_performance", overall.to_py());
        save_metadata(&node_dir, &metadata)?;
        Ok(tasks
            .iter()
            .map(|t| f64::from(u8::from(overall.total_resolved_ids.contains(t))))
            .collect())
    }
}
