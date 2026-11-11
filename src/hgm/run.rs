use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use hgm_agent::llm::Service;
use hgm_bench::dataset::{Hub, SWE_BENCH_VERIFIED, instance_ids};
use hgm_bench::polyglot::{self, DATASET};
use hgm_bench::polyglot_build::copy_tree;
use hgm_bench::polyglot_prepare::prepare;
use hgm_bench::polyglot_spec::NAMESPACE;
use hgm_bench::swe;
use hgm_bench::tasks::{dedup, polyglot_run_tasks, shuffled, swe_run_tasks};
use hgm_core::config::HgmConfig;
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

use crate::agent_build::Builder;
use crate::best::{BEST_AGENT, check_epsilon, export, pick};
use crate::cli::{Cli, Tools};
use crate::improve::SelfImprove;
use crate::node::{Bench, Evaluator, at};
use crate::search::{Search, Settings, State};
use crate::seed;

pub const JITTER: Duration = Duration::from_secs(1);

pub const HOLDOUT_IMAGE: &str = "hgm-holdout-agent:latest";

pub const HOLDOUT_DIR: &str = "holdout";

fn builder(docker: &Docker, context: &Path, agent_name: &str) -> Builder {
    Builder {
        docker: docker.clone(),
        context: context.to_path_buf(),
        image_name: format!("{agent_name}:latest"),
        rebuild: false,
    }
}

pub fn initial_folder(polyglot: bool) -> &'static str {
    if polyglot {
        "initial_polyglot"
    } else {
        "initial_swe"
    }
}

pub fn resolve_output_dir(output_dir: &str, cwd: &Path) -> (PathBuf, String) {
    let dir = cwd.join(output_dir);
    let run_id = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (dir, run_id)
}

pub struct Init<'a> {
    pub seed_dir: &'a Path,
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

fn build_seed(src_path: &Path, docker: &Docker, log: &Log) -> Result<(), hgm_sandbox::Error> {
    let build = tempfile::tempdir()?;
    seed::write(build.path())?;
    prepare_initial_agent(build.path(), src_path, Some(docker), log).map(|_| ())
}

pub fn initialize_run(
    init: &Init,
    bench: &dyn Bench,
    docker: &Docker,
    log: &Log,
) -> Result<Start, String> {
    let folder = init.seed_dir.join(initial_folder(init.polyglot));
    let agent_dir = folder.join(init.agent_name);
    let src_path = agent_dir.join("src");
    if init.continue_from.is_none() && !agent_dir.exists() {
        build_seed(&src_path, docker, log).map_err(|e| e.to_string())?;
        Evaluator {
            bench,
            output_dir: &folder,
            max_workers: init.max_workers,
            builder: &builder(docker, &src_path, init.agent_name),
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

pub fn improve_dataset(
    polyglot: bool,
    polyglot_dir: &Path,
    hub: &Hub,
) -> Result<Vec<Value>, String> {
    if polyglot {
        let path = polyglot_dir.join(DATASET);
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
    service: &Service,
    polyglot_dir: &Path,
    tools: &Tools,
) -> Box<dyn Bench> {
    let llm = config.llm.downstream_llm.clone();
    let timeout = config.execution.evaluation_timeout;
    if config.evaluation.polyglot {
        Box::new(polyglot::Harness {
            docker: tools.docker.clone(),
            root: polyglot_dir.to_path_buf(),
            dataset: polyglot_dir.join(DATASET),
            machine: std::env::consts::ARCH.to_owned(),
            namespace: NAMESPACE.to_owned(),
            llm,
            service: service.clone(),
            timeout,
        })
    } else {
        Box::new(swe::Harness {
            docker: tools.docker.clone(),
            swebench: tools.swebench.clone(),
            hub: tools.hub.clone(),
            llm,
            service: service.clone(),
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
        expansion_statistic: config.optimization.expansion_statistic,
        eval_random_level: config.optimization.eval_random_level,
        jitter,
    }
}

pub fn run(args: &Cli, root_dir: &Path, jitter: Duration) -> Result<(), String> {
    let config = args.config();
    if config.paths.initial_agent_name.is_empty() {
        return Err("--initial_agent_name must not be empty".to_owned());
    }
    check_epsilon(args.epsilon)?;
    if config.evaluation.polyglot && args.holdout_split.is_some() {
        return Err(
            "--holdout_split scores on SWE-bench, so it cannot follow a --polyglot run".to_owned(),
        );
    }
    let services = args.services(root_dir)?;
    let polyglot_dir = root_dir.join(&args.polyglot_dir);
    if config.evaluation.polyglot {
        prepare_polyglot(&polyglot_dir, &args.polyglot_repo)?;
    }
    let (output_dir, run_id) = resolve_output_dir(&config.paths.output_dir, root_dir);
    fs::create_dir_all(&output_dir).map_err(|e| at(&output_dir, e))?;
    println!("Working directory: {}", root_dir.display());
    println!("Output directory: {}", output_dir.display());
    println!("Output directory exists: {}", output_dir.exists());
    let outer = output_dir.join("hgm_outer.log");
    let log = Log::to_file(&outer).map_err(|e| at(&outer, e))?;
    let tools = args.tools(root_dir);
    let tasks = dedup(&run_tasks(&config, &tools.hub)?);
    let bench = bench_for(&config, &services.downstream, &polyglot_dir, &tools);
    let polyglot = config.evaluation.polyglot;
    let max_workers = config.execution.max_workers;
    let continue_from = config
        .paths
        .continue_from
        .as_ref()
        .map(|p| root_dir.join(p));
    let start = initialize_run(
        &Init {
            seed_dir: &root_dir.join(&args.seed_dir),
            output_dir: &output_dir,
            agent_name: &config.paths.initial_agent_name,
            polyglot,
            continue_from: continue_from.as_deref(),
            total_tasks: &tasks,
            max_workers,
        },
        bench.as_ref(),
        &tools.docker,
        &log,
    )?;
    log.info(format!(
        "Starting HGM run {run_id} with configuration: {}",
        config.to_json()
    ));
    let improver = SelfImprove {
        docker: tools.docker.clone(),
        agent_source: start.src_path.clone(),
        output_dir: output_dir.clone(),
        polyglot,
        dataset: improve_dataset(polyglot, &polyglot_dir, &tools.hub),
        image_name: format!("{}:latest", config.paths.initial_agent_name),
        diagnose: services
            .diagnose
            .client(&config.llm.diagnose_llm, &services.api_key),
        self_improve_llm: config.llm.self_improve_llm.clone(),
        self_improve_service: services.self_improve.clone(),
        timeout: config.execution.self_improve_timeout,
    };
    let builder = builder(
        &improver.docker,
        &start.src_path,
        &config.paths.initial_agent_name,
    );
    let evaluator = Evaluator {
        bench: bench.as_ref(),
        output_dir: &output_dir,
        max_workers,
        builder: &builder,
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
    search.run()?;
    finish(args, &config, &services.downstream, &tools, &output_dir)
}

pub fn finish(
    args: &Cli,
    config: &HgmConfig,
    downstream: &Service,
    tools: &Tools,
    output_dir: &Path,
) -> Result<(), String> {
    let best = pick(output_dir, args.epsilon)?;
    let agent_dir = output_dir.join(BEST_AGENT);
    export(output_dir, &best.commit_id, &agent_dir)?;
    println!(
        "Best agent: {} solved {}/{} (score {})",
        best.commit_id, best.successes, best.evals, best.quantile
    );
    println!("Best agent source: {}", agent_dir.display());
    match (&args.holdout_split, args.holdout_tasks) {
        (Some(split), Some(n_tasks)) => holdout(&Holdout {
            agent_dir: &agent_dir,
            results_dir: &output_dir.join(HOLDOUT_DIR),
            split,
            n_tasks,
            llm: &config.llm.downstream_llm,
            service: downstream,
            tools,
            timeout: config.execution.evaluation_timeout,
            max_workers: config.execution.max_workers,
        }),
        _ => Ok(()),
    }
}

pub struct Holdout<'a> {
    pub agent_dir: &'a Path,
    pub results_dir: &'a Path,
    pub split: &'a str,
    pub n_tasks: usize,
    pub llm: &'a str,
    pub service: &'a Service,
    pub tools: &'a Tools,
    pub timeout: u64,
    pub max_workers: usize,
}

pub fn holdout(h: &Holdout) -> Result<(), String> {
    let hub = &h.tools.hub;
    let dataset = format!("princeton-nlp/SWE-bench_{}", h.split);
    let mut ids = hub
        .load(&dataset, "test")
        .map(|rows| instance_ids(&rows))
        .map_err(|e| e.to_string())?;
    ids.truncate(h.n_tasks);
    let agent_name = h
        .agent_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let node_dir = h.results_dir.join(&agent_name);
    fs::create_dir_all(&node_dir).map_err(|e| at(&node_dir, e))?;
    let bench = swe::Harness {
        docker: h.tools.docker.clone(),
        swebench: h.tools.swebench.clone(),
        hub: hub.clone(),
        llm: h.llm.to_owned(),
        service: h.service.clone(),
        timeout: h.timeout,
    };
    let builder = Builder {
        docker: bench.docker.clone(),
        context: h.agent_dir.to_path_buf(),
        image_name: HOLDOUT_IMAGE.to_owned(),
        rebuild: true,
    };
    let solved = Evaluator {
        bench: &bench,
        output_dir: h.results_dir,
        max_workers: h.max_workers,
        builder: &builder,
    }
    .eval_agent(&agent_name, &ids)?;
    println!(
        "Held-out {}: solved {}/{}",
        h.split,
        solved.iter().filter(|s| **s > 0.0).count(),
        ids.len()
    );
    Ok(())
}

pub fn prepare_polyglot(polyglot_dir: &Path, repo_url: &str) -> Result<(), String> {
    prepare(polyglot_dir, repo_url).map(|_| ())
}
