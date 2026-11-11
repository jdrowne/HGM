use std::fs;
use std::path::{Path, PathBuf};

use hgm_coding_agent::patch::remove_patches_to_tests;
use hgm_core::pyjson::{PyValue, dumps};
use serde_json::{Map, Value};

use crate::swebench::{EvaluationRun, Swebench};

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

pub fn load_predictions(paths: &[PathBuf]) -> Result<Map<String, Value>, String> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push(path.clone());
        } else if path.is_dir() {
            files.extend(json_files(path));
        } else {
            return Err(format!("{} does not exist", path.display()));
        }
    }
    let mut predictions = Map::new();
    for file in files {
        let mut pred: Value = fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .map_err(|e| format!("{}: {e}", file.display()))?;
        let Some(inst) = pred
            .get("instance_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            println!("Skipping json without instance_id {}", file.display());
            continue;
        };
        pred["json_fname"] = Value::String(file.to_string_lossy().into_owned());
        predictions.insert(inst, pred);
    }
    Ok(predictions)
}

pub fn preds_to_jsonl(dname: &Path, predictions: &Map<String, Value>) -> Result<PathBuf, String> {
    let model = predictions
        .values()
        .next()
        .and_then(|p| p["model_name_or_path"].as_str())
        .ok_or_else(|| format!("no predictions in {}", dname.display()))?;
    let mut out = String::new();
    for pred in predictions.values() {
        if pred["model_name_or_path"].as_str() != Some(model) {
            return Err(format!(
                "{} mixes predictions from {model} and {}",
                dname.display(),
                pred["model_name_or_path"]
            ));
        }
        let minimal = PyValue::dict([
            ("model_name_or_path", PyValue::str(model)),
            (
                "model_patch",
                PyValue::str(remove_patches_to_tests(
                    pred["model_patch"].as_str().unwrap_or_default(),
                )),
            ),
            ("instance_id", PyValue::from(&pred["instance_id"])),
        ]);
        out.push_str(&dumps(&minimal, None));
        out.push('\n');
    }
    let path = dname.join("all_preds.jsonl");
    fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[derive(Clone, Debug)]
pub struct Report {
    pub dataset_name: String,
    pub output_dir: PathBuf,
    pub dnames_workers: Option<usize>,
    pub num_eval_procs: usize,
}

impl Default for Report {
    fn default() -> Self {
        Self {
            dataset_name: crate::dataset::SWE_BENCH.to_owned(),
            output_dir: PathBuf::from("./swe_bench"),
            dnames_workers: None,
            num_eval_procs: 5,
        }
    }
}

pub fn default_run_ids(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{i:03}")).collect()
}

impl Report {
    fn single(&self, swebench: &Swebench, dname: &Path, run_id: &str) -> Result<(), String> {
        let dname = std::path::absolute(dname).map_err(|e| e.to_string())?;
        let predictions = load_predictions(std::slice::from_ref(&dname))?;
        let predictions_path = preds_to_jsonl(&dname, &predictions)?;
        swebench
            .run_evaluation(&EvaluationRun {
                dataset_name: self.dataset_name.clone(),
                predictions_path,
                max_workers: self.num_eval_procs,
                run_id: run_id.to_owned(),
                output_dir: self.output_dir.clone(),
            })
            .map_err(|e| e.to_string())?;
        println!("Report generated for {}", dname.display());
        Ok(())
    }

    pub fn make(
        &self,
        swebench: &Swebench,
        dnames: &[PathBuf],
        run_ids: Option<&[String]>,
    ) -> Vec<Result<(), String>> {
        let run_ids = match run_ids {
            Some(ids) if ids.len() == dnames.len() => ids.to_vec(),
            _ => default_run_ids(dnames.len()),
        };
        let workers = self.dnames_workers.unwrap_or(dnames.len());
        let jobs: Vec<(&PathBuf, &String)> = dnames.iter().zip(&run_ids).collect();
        let results = hgm_core::pool::map(&jobs, workers, |(dname, run_id)| {
            let result = self.single(swebench, dname, run_id);
            if let Err(e) = &result {
                println!("Report failed for {}: {e}", dname.display());
            }
            result
        });
        println!("All reports generated.");
        results
    }
}
