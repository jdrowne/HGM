use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use chrono::Local;
use hgm_agent::llm::create_client;
use hgm_bench::dataset::{Hub, SWE_BENCH_VERIFIED, instance_ids};
use hgm_bench::polyglot::{self, DATASET};
use hgm_bench::polyglot_build::copy_tree;
use hgm_bench::polyglot_prepare::{REPO_URL, prepare};
use hgm_bench::swe;
use hgm_bench::swebench::Swebench;
use hgm_bench::tasks::{dedup, polyglot_run_tasks, shuffled, swe_run_tasks};
use hgm_core::config::{HgmConfig, PathConfig};
use hgm_core::log::Log;
use hgm_core::metadata::{
    HGM_METADATA_FILE, INIT_EVALUATED_TASKS_FILE, load_init_evaluated_tasks,
    load_last_hgm_metadata, read_overall_performance, tree_from_record,
};
use hgm_core::tree::{ROOT_COMMIT, Tree};
use hgm_sandbox::Docker;
use hgm_sandbox::source::prepare_initial_agent;
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde_json::Value;

use crate::cli::{EvaluateArgs, RunArgs};
use crate::improve::SelfImprove;
use crate::node::{Bench, Evaluator, at};
use crate::search::{Search, Settings, State};

pub const JITTER: Duration = Duration::from_secs(1);

pub fn initial_folder(polyglot: bool) -> &'static str {
    if polyglot {
        "initial_polyglot"
    } else {
        "initial_swe"
    }
}

pub fn resolve_output_dir(paths: &PathConfig, cwd: &Path, now: &str) -> (PathBuf, String) {
    let dir = match (&paths.output_dir, &paths.continue_from) {
        (Some(dir), _) => cwd.join(dir),
        (None, None) => cwd.join("output_hgm").join(now),
        (None, Some(prev)) => cwd
            .join("output_hgm")
            .join(Path::new(prev).file_name().unwrap_or_default()),
    };
    let run_id = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (dir, run_id)
}

pub struct Init<'a> {
    pub root_dir: &'a Path,
    pub output_dir: &'a Path,
    pub agent_name: &'a str,
    pub polyglot: bool,
    pub continue_from: Option<&'a Path>,
    pub total_tasks: &'a [String],
    pub max_workers: usize,
}

pub struct Start {
    pub src_path: PathBuf,
    pub tree: Tree,
    pub submitted_ids: HashMap<usize, HashSet<String>>,
    pub n_task_evals: usize,
    pub init_evaluated_tasks: Vec<String>,
}

fn previous_run(prev: &Path) -> Result<(Vec<String>, Tree), String> {
    let tasks = prev.join(INIT_EVALUATED_TASKS_FILE);
    let record = prev.join(HGM_METADATA_FILE);
    let init_evaluated = load_init_evaluated_tasks(&tasks).map_err(|e| at(&tasks, e))?;
    load_last_hgm_metadata(&record)
        .and_then(|last| tree_from_record(&last))
        .map(|tree| (init_evaluated, tree))
        .map_err(|e| at(&record, e))
}

pub fn initialize_run(
    init: &Init,
    bench: &dyn Bench,
    docker: &Docker,
    log: &Log,
) -> Result<Start, String> {
    let folder = init.root_dir.join(initial_folder(init.polyglot));
    let agent_dir = folder.join(init.agent_name);
    let src_path = agent_dir.join("src");
    if init.continue_from.is_none() && !agent_dir.exists() {
        prepare_initial_agent(init.root_dir, &src_path, Some(docker), log)
            .map_err(|e| e.to_string())?;
        Evaluator {
            bench,
            output_dir: &folder,
            max_workers: init.max_workers,
            init_agent_path: &src_path,
        }
        .eval_agent(init.agent_name, init.total_tasks)?;
    }
    copy_tree(&agent_dir, &init.output_dir.join(ROOT_COMMIT))
        .unwrap_or_else(|e| eprintln!("cp: {}: {e}", agent_dir.display()));
    let (init_evaluated_tasks, mut tree) = match init.continue_from {
        Some(prev) => previous_run(prev)?,
        None => (Vec::new(), Tree::with_root()),
    };
    let ids: Vec<usize> = tree.nodes().map(|n| n.id).collect();
    let mut submitted_ids = HashMap::new();
    let mut n_task_evals = 0;
    for id in ids {
        let commit = tree
            .get(id)
            .map(|n| n.commit_id.clone())
            .unwrap_or_default();
        let node_dir = init.output_dir.join(&commit);
        let overall = read_overall_performance(&node_dir).map_err(|e| at(&node_dir, e))?;
        submitted_ids.insert(id, overall.total_submitted_ids.iter().cloned().collect());
        tree.get_mut(id)
            .into_iter()
            .for_each(|n| n.utility_measures = overall.utility_measures());
        if commit != ROOT_COMMIT {
            n_task_evals += overall.total_submitted_instances as usize;
        }
    }
    Ok(Start {
        src_path,
        tree,
        submitted_ids,
        n_task_evals,
        init_evaluated_tasks,
    })
}

pub fn run_tasks(config: &HgmConfig, hub: &Hub) -> Result<Vec<String>, String> {
    if config.evaluation.polyglot {
        Ok(polyglot_run_tasks())
    } else if config.evaluation.full_eval {
        hub.load(SWE_BENCH_VERIFIED, "test")
            .map(|rows| shuffled(instance_ids(&rows)))
            .map_err(|e| e.to_string())
    } else {
        Ok(swe_run_tasks())
    }
}

pub fn improve_dataset(polyglot: bool, root_dir: &Path, hub: &Hub) -> Result<Vec<Value>, String> {
    if polyglot {
        let path = root_dir.join(DATASET);
        fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .map_err(|e| at(&path, e))
    } else {
        hub.load(SWE_BENCH_VERIFIED, "test")
            .map_err(|e| e.to_string())
    }
}

pub fn bench_for(
    config: &HgmConfig,
    root_dir: &Path,
    docker: &Docker,
    hub: &Hub,
) -> Box<dyn Bench> {
    let llm = config.llm.downstream_llm.clone();
    let timeout = config.execution.evaluation_timeout;
    if config.evaluation.polyglot {
        Box::new(polyglot::Harness {
            docker: docker.clone(),
            root: root_dir.to_path_buf(),
            dataset: root_dir.join(DATASET),
            machine: std::env::consts::ARCH.to_owned(),
            llm,
            timeout,
        })
    } else {
        Box::new(swe::Harness {
            docker: docker.clone(),
            swebench: Swebench::from_env(),
            hub: hub.clone(),
            llm,
            timeout,
        })
    }
}

pub fn settings(config: &HgmConfig, jitter: Duration) -> Settings {
    Settings {
        max_task_evals: config.execution.max_task_evals,
        max_workers: config.execution.max_workers,
        alpha: config.optimization.alpha,
        beta: config.optimization.beta,
        cool_down: config.optimization.cool_down,
        n_pseudo_descendant_evals: config.optimization.n_pseudo_descendant_evals,
        eval_random_level: config.optimization.eval_random_level,
        jitter,
    }
}

pub fn run(args: &RunArgs, root_dir: &Path, jitter: Duration) -> Result<(), String> {
    let config = HgmConfig::load(&args.config, &args.overrides());
    if config.paths.initial_agent_name.is_empty() {
        return Err("Initial agent name must be provided either in config.yaml or via --initial_agent_name.".to_owned());
    }
    let now = Local::now().format("%Y%m%d%H%M%S_%6f").to_string();
    let (output_dir, run_id) = resolve_output_dir(&config.paths, root_dir, &now);
    fs::create_dir_all(&output_dir).map_err(|e| at(&output_dir, e))?;
    println!("Working directory: {}", root_dir.display());
    println!("Using config file: {}", args.config.display());
    println!("Output directory: {}", output_dir.display());
    println!("Output directory exists: {}", output_dir.exists());
    let outer = output_dir.join("hgm_outer.log");
    let log = Log::to_file(&outer).map_err(|e| at(&outer, e))?;
    let hub = Hub::from_env();
    let tasks = dedup(&run_tasks(&config, &hub)?);
    let docker = Docker::default();
    let bench = bench_for(&config, root_dir, &docker, &hub);
    let polyglot = config.evaluation.polyglot;
    let max_workers = config.execution.max_workers;
    let continue_from = config
        .paths
        .continue_from
        .as_ref()
        .map(|p| root_dir.join(p));
    let start = initialize_run(
        &Init {
            root_dir,
            output_dir: &output_dir,
            agent_name: &config.paths.initial_agent_name,
            polyglot,
            continue_from: continue_from.as_deref(),
            total_tasks: &tasks,
            max_workers,
        },
        bench.as_ref(),
        &docker,
        &log,
    )?;
    log.info(format!(
        "Starting HGM run {run_id} with configuration: {}",
        config.to_json()
    ));
    let improver = SelfImprove {
        docker,
        root_dir: root_dir.to_path_buf(),
        output_dir: output_dir.clone(),
        polyglot,
        dataset: improve_dataset(polyglot, root_dir, &hub),
        image_name: format!("{}:latest", config.paths.initial_agent_name),
        diagnose: create_client(&config.llm.diagnose_llm),
        self_improve_llm: config.llm.self_improve_llm.clone(),
        timeout: config.execution.self_improve_timeout,
    };
    let evaluator = Evaluator {
        bench: bench.as_ref(),
        output_dir: &output_dir,
        max_workers,
        init_agent_path: &start.src_path,
    };
    let evaluate = |commit: &str, tasks: &[String]| evaluator.eval_agent(commit, tasks);
    let search = Search {
        settings: settings(&config, jitter),
        state: Mutex::new(State {
            tree: start.tree,
            submitted_ids: start.submitted_ids,
            n_task_evals: start.n_task_evals,
            init_evaluated_tasks: start.init_evaluated_tasks,
            n_pending_expands: 0,
            rng: StdRng::from_rng(&mut rand::rng()),
        }),
        output_dir: &output_dir,
        total_tasks: &tasks,
        improver: &improver,
        evaluate: &evaluate,
        log: &log,
    };
    search.run()
}

pub fn evaluate(args: &EvaluateArgs, root_dir: &Path) -> Result<(), String> {
    let hub = Hub::from_env();
    let dataset = format!("princeton-nlp/SWE-bench_{}", args.split);
    let mut ids = hub
        .load(&dataset, "test")
        .map(|rows| instance_ids(&rows))
        .map_err(|e| e.to_string())?;
    if let Some(n) = args.n_tasks {
        ids.truncate(n);
    }
    let results_dir = root_dir.join(&args.results_dir);
    let agent_name = args
        .agent_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let node_dir = results_dir.join(&agent_name);
    fs::create_dir_all(&node_dir).map_err(|e| at(&node_dir, e))?;
    let bench = swe::Harness {
        docker: Docker::default(),
        swebench: Swebench::from_env(),
        hub,
        llm: args.llm.clone(),
        timeout: args.hours_per_task * 3600,
    };
    Evaluator {
        bench: &bench,
        output_dir: &results_dir,
        max_workers: args.num_workers,
        init_agent_path: &root_dir.join(&args.agent_path),
    }
    .eval_agent(&agent_name, &ids)
    .map(drop)
}

pub fn prepare_polyglot(root_dir: &Path) -> Result<(), String> {
    let url = std::env::var("HGM_POLYGLOT_REPO").ok();
    prepare(root_dir, url.as_deref().unwrap_or(REPO_URL)).map(|_| ())
}
