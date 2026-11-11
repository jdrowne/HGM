use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::pyjson::{self, PyValue};
use crate::tree::{ROOT_COMMIT, Tree};

pub const HGM_METADATA_FILE: &str = "rustyhgm_metadata.jsonl";

pub const INIT_EVALUATED_TASKS_FILE: &str = "init_evaluated_tasks.json";

pub const NODE_METADATA_FILE: &str = "metadata.json";

pub const MODEL_PATCH_FILE: &str = "model_patch.diff";

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

pub fn record(tree: &Tree, n_task_evals: usize) -> PyValue {
    let nodes = tree
        .nodes()
        .filter(|n| n.commit_id != ROOT_COMMIT)
        .map(|n| n.save_as_dict())
        .collect();
    PyValue::dict([
        ("n_task_evals", PyValue::Int(n_task_evals as i64)),
        ("nodes", PyValue::List(nodes)),
    ])
}

pub fn update_metadata(
    output_dir: &Path,
    tree: &Tree,
    n_task_evals: usize,
    init_evaluated_tasks: &[String],
) -> io::Result<()> {
    let mut line = pyjson::dumps(&record(tree, n_task_evals), Some(2));
    line.push('\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(output_dir.join(HGM_METADATA_FILE))
        .and_then(|mut jsonl| jsonl.write_all(line.as_bytes()))?;
    fs::write(
        output_dir.join(INIT_EVALUATED_TASKS_FILE),
        pyjson::dumps(&PyValue::list_of_str(init_evaluated_tasks), None),
    )
}

fn parse_entry(entry: &str) -> io::Result<Value> {
    let entry = if entry.starts_with('{') {
        entry.to_string()
    } else {
        format!("{{{entry}")
    };
    pyjson::parse_lenient(&entry).map_err(invalid)
}

pub fn parse_rustyhgm_metadata(content: &str) -> io::Result<Vec<Value>> {
    content.trim().split("\n{").map(parse_entry).collect()
}

pub fn load_rustyhgm_metadata(path: &Path) -> io::Result<Vec<Value>> {
    parse_rustyhgm_metadata(&fs::read_to_string(path)?)
}

pub fn load_last_rustyhgm_metadata(path: &Path) -> io::Result<Value> {
    let content = fs::read_to_string(path)?;
    parse_entry(content.trim().rsplit("\n{").next().unwrap_or_default())
}

pub fn load_init_evaluated_tasks(path: &Path) -> io::Result<Vec<String>> {
    serde_json::from_str(&fs::read_to_string(path)?).map_err(invalid)
}

pub fn tree_from_record(record: &Value) -> io::Result<Tree> {
    let mut tree = Tree::with_root();
    let nodes = record
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("record has no nodes list"))?;
    for node in nodes {
        let commit_id = node
            .get("commit_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("node has no commit_id"))?;
        let id = node
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("node has no id"))?;
        let parent_id = node
            .get("parent_id")
            .and_then(Value::as_u64)
            .map(|p| p as usize);
        tree.insert(commit_id, parent_id, Some(id as usize));
    }
    tree.link_children();
    Ok(tree)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverallPerformance {
    pub total_resolved_instances: u64,
    pub total_submitted_instances: u64,
    pub files: Vec<String>,
    pub total_unresolved_ids: Vec<String>,
    pub total_emptypatch_ids: Vec<String>,
    pub total_resolved_ids: Vec<String>,
    pub total_submitted_ids: Vec<String>,
}

fn ratio(resolved: u64, submitted: u64) -> PyValue {
    if submitted > 0 {
        PyValue::Float(resolved as f64 / submitted as f64)
    } else {
        PyValue::Int(0)
    }
}

fn count(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn ids(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

impl OverallPerformance {
    pub fn accuracy_score(&self) -> f64 {
        if self.total_submitted_instances > 0 {
            self.total_resolved_instances as f64 / self.total_submitted_instances as f64
        } else {
            0.0
        }
    }

    pub fn from_value(v: &Value) -> Self {
        Self {
            total_resolved_instances: count(v, "total_resolved_instances"),
            total_submitted_instances: count(v, "total_submitted_instances"),
            files: ids(v, "files"),
            total_unresolved_ids: ids(v, "total_unresolved_ids"),
            total_emptypatch_ids: ids(v, "total_emptypatch_ids"),
            total_resolved_ids: ids(v, "total_resolved_ids"),
            total_submitted_ids: ids(v, "total_submitted_ids"),
        }
    }

    pub fn to_py(&self) -> PyValue {
        PyValue::dict([
            (
                "accuracy_score",
                ratio(
                    self.total_resolved_instances,
                    self.total_submitted_instances,
                ),
            ),
            (
                "total_resolved_instances",
                PyValue::Int(self.total_resolved_instances as i64),
            ),
            (
                "total_submitted_instances",
                PyValue::Int(self.total_submitted_instances as i64),
            ),
            ("files", PyValue::list_of_str(&self.files)),
            (
                "total_unresolved_ids",
                PyValue::list_of_str(&self.total_unresolved_ids),
            ),
            (
                "total_emptypatch_ids",
                PyValue::list_of_str(&self.total_emptypatch_ids),
            ),
            (
                "total_resolved_ids",
                PyValue::list_of_str(&self.total_resolved_ids),
            ),
            (
                "total_submitted_ids",
                PyValue::list_of_str(&self.total_submitted_ids),
            ),
        ])
    }

    pub fn utility_measures(&self) -> Vec<f64> {
        let resolved = self.total_resolved_instances as usize;
        let unresolved = self
            .total_submitted_instances
            .saturating_sub(self.total_resolved_instances) as usize;
        let mut evals = vec![1.0; resolved];
        evals.resize(resolved + unresolved, 0.0);
        evals
    }
}

pub fn read_overall_performance(node_dir: &Path) -> io::Result<OverallPerformance> {
    let text = fs::read_to_string(node_dir.join(NODE_METADATA_FILE))?;
    let metadata: Value = pyjson::parse_lenient(&text).map_err(invalid)?;
    let overall = metadata
        .get("overall_performance")
        .ok_or_else(|| invalid("metadata.json has no overall_performance"))?;
    Ok(OverallPerformance::from_value(overall))
}

pub fn get_all_performance(
    results_dir: &Path,
    run_keyword: &str,
) -> io::Result<(Vec<PyValue>, OverallPerformance)> {
    let mut matching: Vec<String> = fs::read_dir(results_dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json") && name.contains(run_keyword))
        .collect();
    matching.sort();
    let mut overall = OverallPerformance {
        files: matching.clone(),
        ..Default::default()
    };
    let mut per_file = Vec::new();
    for name in &matching {
        let text = fs::read_to_string(results_dir.join(name))?;
        let results: Value = pyjson::parse_lenient(&text).map_err(invalid)?;
        let resolved = count(&results, "resolved_instances");
        let submitted = count(&results, "submitted_instances");
        overall.total_resolved_instances += resolved;
        overall.total_submitted_instances += submitted;
        let mut entry = vec![
            ("file".to_string(), PyValue::str(name)),
            ("accuracy_score".to_string(), ratio(resolved, submitted)),
        ];
        if let Value::Object(fields) = &results {
            for (k, v) in fields {
                let v = PyValue::from(v);
                match entry.iter_mut().find(|(existing, _)| existing == k) {
                    Some(slot) => slot.1 = v,
                    None => entry.push((k.clone(), v)),
                }
            }
        }
        per_file.push(PyValue::Dict(entry));
        overall
            .total_unresolved_ids
            .extend(ids(&results, "unresolved_ids"));
        overall
            .total_emptypatch_ids
            .extend(ids(&results, "empty_patch_ids"));
        overall
            .total_resolved_ids
            .extend(ids(&results, "resolved_ids"));
        overall
            .total_submitted_ids
            .extend(ids(&results, "submitted_ids"));
    }
    Ok((per_file, overall))
}

pub fn model_patch_paths(
    root_dir: &Path,
    rustyhgm_dir: &str,
    parent_commit: &str,
) -> io::Result<Vec<PathBuf>> {
    let mut commit = parent_commit.to_string();
    let mut patches = Vec::new();
    while commit != ROOT_COMMIT {
        let dir = root_dir.join(rustyhgm_dir).join(&commit);
        let patch = dir.join(MODEL_PATCH_FILE);
        if patch.exists() {
            patches.push(patch);
        } else {
            eprintln!("Parent patch file not found: {}", patch.display());
        }
        let text = fs::read_to_string(dir.join(NODE_METADATA_FILE))?;
        let metadata: Value = pyjson::parse_lenient(&text).map_err(invalid)?;
        commit = metadata
            .get("parent_commit")
            .and_then(Value::as_str)
            .unwrap_or(ROOT_COMMIT)
            .to_string();
    }
    patches.reverse();
    Ok(patches)
}

pub fn is_compiled_self_improve(metadata: &Value, num_swe_issues: Option<u64>) -> bool {
    let Some(overall) = metadata
        .get("overall_performance")
        .and_then(Value::as_object)
    else {
        return false;
    };
    let required = [
        "accuracy_score",
        "total_unresolved_ids",
        "total_resolved_ids",
        "total_emptypatch_ids",
    ];
    if overall.is_empty() || !required.iter().all(|k| overall.contains_key(*k)) {
        return false;
    }
    let len =
        |m: &Map<String, Value>, k: &str| m.get(k).and_then(Value::as_array).map_or(0, Vec::len);
    if len(overall, "total_resolved_ids") + len(overall, "total_unresolved_ids") == 0 {
        return false;
    }
    let evaluated = overall
        .get("total_submitted_instances")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    num_swe_issues.is_none_or(|n| evaluated >= n)
}
