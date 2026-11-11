use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use hgm::FAILED;
use hgm::improve::Improver;
use hgm::search::{Search, Settings, State};
use hgm_core::log::Log;
use hgm_core::metadata::load_hgm_metadata;
use hgm_core::tree::Tree;
use rand::SeedableRng;
use rand::rngs::StdRng;

struct Children {
    made: Mutex<Vec<String>>,
    fail: bool,
}

impl Children {
    fn new(fail: bool) -> Self {
        Children {
            made: Mutex::default(),
            fail,
        }
    }
}

impl Improver for Children {
    fn sample_child(&self, parent_commit: &str) -> String {
        if self.fail {
            return FAILED.to_owned();
        }
        let mut made = self.made.lock().unwrap();
        let child = format!("{parent_commit}.{}", made.len());
        made.push(child.clone());
        child
    }
}

fn settings(max_task_evals: usize) -> Settings {
    Settings {
        max_task_evals,
        max_workers: 2,
        alpha: 0.6,
        beta: 1.0,
        cool_down: false,
        n_pseudo_descendant_evals: 10000,
        eval_random_level: 0.0,
        jitter: Duration::ZERO,
    }
}

fn start_state(root_evals: &[f64]) -> State {
    let mut tree = Tree::with_root();
    tree.get_mut(0).unwrap().utility_measures = root_evals.to_vec();
    State {
        tree,
        submitted_ids: HashMap::new(),
        n_task_evals: 0,
        init_evaluated_tasks: vec!["t0".to_owned()],
        n_pending_expands: 0,
        rng: StdRng::seed_from_u64(7),
    }
}

fn tasks(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("t{i}")).collect()
}

struct Run<'a> {
    dir: &'a Path,
    tasks: &'a [String],
    improver: &'a dyn Improver,
}

fn searching<'a>(
    run: &Run<'a>,
    settings: Settings,
    state: State,
    evaluate: &'a hgm::search::Evaluate<'a>,
    log: &'a Log,
) -> Search<'a> {
    Search {
        settings,
        state: Mutex::new(state),
        output_dir: run.dir,
        total_tasks: run.tasks,
        improver: run.improver,
        evaluate,
        log,
    }
}

#[test]
fn a_run_expands_then_spends_the_budget_and_records_each_step() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tasks(20);
    let children = Children::new(false);
    let calls = Mutex::new(Vec::new());
    let evaluate = |commit: &str, tasks: &[String]| {
        calls
            .lock()
            .unwrap()
            .push((commit.to_owned(), tasks.to_vec()));
        Ok(vec![1.0])
    };
    let log = Log::discard();
    let run = Run {
        dir: tmp.path(),
        tasks: &tasks,
        improver: &children,
    };
    let mut s = settings(6);
    s.cool_down = true;
    let search = searching(&run, s, start_state(&[1.0]), &evaluate, &log);
    search.run().unwrap();
    let state = search.state.into_inner().unwrap();
    assert_eq!(state.n_task_evals, 6);
    assert_eq!(calls.lock().unwrap().len(), 6);
    assert!(state.tree.len() > 2, "{:?}", state.tree);
    let made = children.made.lock().unwrap().clone();
    assert_eq!(state.tree.len(), made.len() + 1);
    let records = load_hgm_metadata(&tmp.path().join("hgm_metadata.jsonl")).unwrap();
    assert_eq!(records.len(), made.len() + 6);
    assert_eq!(records.last().unwrap()["n_task_evals"], 6);
    assert_eq!(
        fs::read_to_string(tmp.path().join("init_evaluated_tasks.json")).unwrap(),
        "[\"t0\"]"
    );
    for (_, evaluated) in calls.lock().unwrap().iter() {
        assert_eq!(evaluated.len(), 1);
    }
    let submitted: usize = state.submitted_ids.values().map(HashSet::len).sum();
    assert_eq!(submitted, 6);
    assert_eq!(state.n_pending_expands, 0);
}

#[test]
fn failed_children_are_not_added() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tasks(2);
    let children = Children::new(true);
    let evaluate = |_: &str, _: &[String]| Ok(vec![0.0]);
    let log = Log::discard();
    let run = Run {
        dir: tmp.path(),
        tasks: &tasks,
        improver: &children,
    };
    let mut s = settings(1);
    s.max_workers = 1;
    let search = searching(&run, s, start_state(&[1.0]), &evaluate, &log);
    search.run().unwrap();
    let state = search.state.into_inner().unwrap();
    assert_eq!(state.tree.len(), 1);
    assert_eq!(state.n_task_evals, 0);
    assert_eq!(state.tree.get(0).unwrap().utility_measures, [1.0]);
    assert!(!tmp.path().join("hgm_metadata.jsonl").exists());
}

#[test]
fn a_run_with_no_expandable_node_stops_after_the_initial_expansions() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tasks(2);
    let children = Children::new(false);
    let evaluate = |_: &str, _: &[String]| Ok(vec![0.0]);
    let log_path = tmp.path().join("outer.log");
    let log = Log::to_file(&log_path).unwrap();
    let run = Run {
        dir: tmp.path(),
        tasks: &tasks,
        improver: &children,
    };
    let search = searching(&run, settings(4), start_state(&[0.0]), &evaluate, &log);
    let e = search.run().unwrap_err();
    assert!(e.starts_with("no node can be expanded"), "{e}");
    assert!(
        fs::read_to_string(&log_path)
            .unwrap()
            .contains("Error: no node can be expanded")
    );
    assert_eq!(search.state.into_inner().unwrap().n_task_evals, 0);
}

#[test]
fn evaluation_and_recording_errors_are_reported_after_the_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tasks(4);
    let children = Children::new(true);
    let evaluate = |_: &str, _: &[String]| Err("eval broke".to_owned());
    let log = Log::discard();
    let run = Run {
        dir: tmp.path(),
        tasks: &tasks,
        improver: &children,
    };
    let mut start = start_state(&[1.0]);
    start.n_task_evals = 1;
    start.tree.insert("x", Some(0), None);
    start.tree.insert("y", Some(0), None);
    let search = searching(&run, settings(3), start, &evaluate, &log);
    assert_eq!(search.run().unwrap_err(), "eval broke");
    let state = search.state.into_inner().unwrap();
    assert_eq!(state.n_task_evals, 3);
    assert_eq!(state.tree.get(0).unwrap().utility_measures, [1.0]);

    let blocked = tmp.path().join("blocked");
    fs::create_dir_all(blocked.join("hgm_metadata.jsonl")).unwrap();
    let ok = |_: &str, _: &[String]| Ok(vec![1.0]);
    let run = Run {
        dir: &blocked,
        tasks: &tasks,
        improver: &children,
    };
    let mut start = start_state(&[1.0]);
    start.n_task_evals = 1;
    start.tree.insert("x", Some(0), None);
    start.tree.insert("y", Some(0), None);
    let search = searching(&run, settings(2), start, &ok, &log);
    assert!(search.run().is_err());
    let growing = Children::new(false);
    let run = Run {
        dir: &blocked,
        tasks: &tasks,
        improver: &growing,
    };
    let search = searching(&run, settings(2), start_state(&[1.0]), &ok, &log);
    assert!(search.run().is_err());
}

#[test]
fn nothing_left_to_evaluate_spends_no_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tasks(1);
    let children = Children::new(true);
    let evaluate = |_: &str, _: &[String]| Ok(vec![1.0]);
    let log = Log::discard();
    let full = Run {
        dir: tmp.path(),
        tasks: &tasks,
        improver: &children,
    };
    let mut start = state_with_all_submitted();
    start.n_task_evals = 1;
    let search = searching(&full, settings(2), start, &evaluate, &log);
    search.run().unwrap();
    assert_eq!(search.state.into_inner().unwrap().n_task_evals, 1);
}

fn state_with_all_submitted() -> State {
    let mut start = start_state(&[1.0]);
    start.tree.insert("x", Some(0), None);
    start
        .submitted_ids
        .insert(0, HashSet::from(["t0".to_owned()]));
    start
        .submitted_ids
        .insert(1, HashSet::from(["t0".to_owned()]));
    start
}

#[test]
fn a_sample_that_wakes_after_the_budget_is_spent_does_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let tasks = tasks(4);
    let children = Children::new(true);
    let calls = Mutex::new(0);
    let evaluate = |_: &str, _: &[String]| {
        *calls.lock().unwrap() += 1;
        Ok(vec![1.0])
    };
    let log = Log::discard();
    let run = Run {
        dir: tmp.path(),
        tasks: &tasks,
        improver: &children,
    };
    let mut start = start_state(&[1.0]);
    start.tree.insert("x", Some(0), None);
    start.tree.insert("y", Some(0), None);
    let mut s = settings(1);
    s.jitter = Duration::from_millis(200);
    let search = searching(&run, s, start, &evaluate, &log);
    search.run().unwrap();
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(search.state.into_inner().unwrap().n_task_evals, 1);
}
