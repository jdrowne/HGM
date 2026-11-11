use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use hgm::FAILED;
use hgm::improve::Improver;
use hgm::search::{Search, Settings, State};
use hgm_core::config::ExpansionStatistic;
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
        expansion_statistic: ExpansionStatistic::Clade,
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

struct Setup<'a> {
    dir: &'a Path,
    tasks: &'a [String],
    improver: &'a dyn Improver,
    log: &'a Log,
}

fn search_in(
    setup: &Setup<'_>,
    settings: Settings,
    state: State,
    evaluate: &hgm::search::Evaluate<'_>,
) -> (Result<(), String>, State) {
    let search = Search {
        settings,
        state: Mutex::new(state),
        output_dir: setup.dir,
        total_tasks: setup.tasks,
        improver: setup.improver,
        evaluate,
        log: setup.log,
    };
    let result = search.run();
    (result, search.state.into_inner().unwrap())
}

struct Big {
    tmp: tempfile::TempDir,
    state: State,
    calls: Vec<(String, Vec<String>)>,
    made: Vec<String>,
}

fn big_run() -> Big {
    let tmp = tempfile::tempdir().unwrap();
    let children = Children::new(false);
    let calls = Mutex::new(Vec::new());
    let evaluate = |commit: &str, tasks: &[String]| {
        calls
            .lock()
            .unwrap()
            .push((commit.to_owned(), tasks.to_vec()));
        Ok(vec![1.0])
    };
    let mut s = settings(6);
    s.cool_down = true;
    let (result, state) = search_in(
        &Setup {
            dir: tmp.path(),
            tasks: &tasks(20),
            improver: &children,
            log: &Log::discard(),
        },
        s,
        start_state(&[1.0]),
        &evaluate,
    );
    result.unwrap();
    Big {
        tmp,
        state,
        calls: calls.into_inner().unwrap(),
        made: children.made.into_inner().unwrap(),
    }
}

#[test]
fn a_run_spends_the_whole_budget() {
    assert_eq!(big_run().state.n_task_evals, 6);
}

#[test]
fn a_run_evaluates_once_per_budget_step() {
    assert_eq!(big_run().calls.len(), 6);
}

#[test]
fn a_run_expands_the_tree() {
    let big = big_run();
    assert!(big.state.tree.len() > 2, "{:?}", big.state.tree);
}

#[test]
fn every_child_made_joins_the_tree() {
    let big = big_run();
    assert_eq!(big.state.tree.len(), big.made.len() + 1);
}

fn records(big: &Big) -> Vec<serde_json::Value> {
    load_hgm_metadata(&big.tmp.path().join("hgm_metadata.jsonl")).unwrap()
}

#[test]
fn a_run_records_each_expansion_and_evaluation() {
    let big = big_run();
    assert_eq!(records(&big).len(), big.made.len() + 6);
}

#[test]
fn the_last_record_holds_the_spent_budget() {
    assert_eq!(records(&big_run()).last().unwrap()["n_task_evals"], 6);
}

#[test]
fn a_run_records_the_initially_evaluated_tasks() {
    assert_eq!(
        fs::read_to_string(big_run().tmp.path().join("init_evaluated_tasks.json")).unwrap(),
        "[\"t0\"]"
    );
}

#[test]
fn each_evaluation_takes_one_task() {
    let sizes: Vec<usize> = big_run()
        .calls
        .iter()
        .map(|(_, evaluated)| evaluated.len())
        .collect();
    assert_eq!(sizes, [1; 6]);
}

#[test]
fn each_evaluation_is_recorded_as_submitted() {
    let submitted: usize = big_run()
        .state
        .submitted_ids
        .values()
        .map(HashSet::len)
        .sum();
    assert_eq!(submitted, 6);
}

#[test]
fn a_finished_run_has_no_pending_expansions() {
    assert_eq!(big_run().state.n_pending_expands, 0);
}

fn failing_children() -> (tempfile::TempDir, State) {
    let tmp = tempfile::tempdir().unwrap();
    let evaluate = |_: &str, _: &[String]| Ok(vec![0.0]);
    let mut s = settings(1);
    s.max_workers = 1;
    let (result, state) = search_in(
        &Setup {
            dir: tmp.path(),
            tasks: &tasks(2),
            improver: &Children::new(true),
            log: &Log::discard(),
        },
        s,
        start_state(&[1.0]),
        &evaluate,
    );
    result.unwrap();
    (tmp, state)
}

#[test]
fn failed_children_are_not_added() {
    assert_eq!(failing_children().1.tree.len(), 1);
}

#[test]
fn failed_children_spend_no_budget() {
    assert_eq!(failing_children().1.n_task_evals, 0);
}

#[test]
fn failed_children_leave_the_root_alone() {
    assert_eq!(
        failing_children().1.tree.get(0).unwrap().utility_measures,
        [1.0]
    );
}

#[test]
fn failed_children_are_not_recorded() {
    let (tmp, _) = failing_children();
    assert!(!tmp.path().join("hgm_metadata.jsonl").exists());
}

fn unexpandable() -> (tempfile::TempDir, Result<(), String>, State) {
    let tmp = tempfile::tempdir().unwrap();
    let evaluate = |_: &str, _: &[String]| Ok(vec![0.0]);
    let log = Log::to_file(&tmp.path().join("outer.log")).unwrap();
    let (result, state) = search_in(
        &Setup {
            dir: tmp.path(),
            tasks: &tasks(2),
            improver: &Children::new(false),
            log: &log,
        },
        settings(4),
        start_state(&[0.0]),
        &evaluate,
    );
    (tmp, result, state)
}

#[test]
fn a_run_with_no_expandable_node_stops_after_the_initial_expansions() {
    let e = unexpandable().1.unwrap_err();
    assert!(e.starts_with("no node can be expanded"), "{e}");
}

#[test]
fn a_run_with_no_expandable_node_logs_why() {
    let (tmp, _, _) = unexpandable();
    assert!(
        fs::read_to_string(tmp.path().join("outer.log"))
            .unwrap()
            .contains("Error: no node can be expanded")
    );
}

#[test]
fn a_run_with_no_expandable_node_spends_no_budget() {
    assert_eq!(unexpandable().2.n_task_evals, 0);
}

fn two_children() -> State {
    let mut start = start_state(&[1.0]);
    start.n_task_evals = 1;
    start.tree.insert("x", Some(0), None);
    start.tree.insert("y", Some(0), None);
    start
}

fn broken_eval() -> (Result<(), String>, State) {
    let tmp = tempfile::tempdir().unwrap();
    let evaluate = |_: &str, _: &[String]| Err("eval broke".to_owned());
    search_in(
        &Setup {
            dir: tmp.path(),
            tasks: &tasks(4),
            improver: &Children::new(true),
            log: &Log::discard(),
        },
        settings(3),
        two_children(),
        &evaluate,
    )
}

#[test]
fn evaluation_errors_are_reported_after_the_budget() {
    assert_eq!(broken_eval().0.unwrap_err(), "eval broke");
}

#[test]
fn failed_evaluations_still_spend_the_budget() {
    assert_eq!(broken_eval().1.n_task_evals, 3);
}

#[test]
fn failed_evaluations_record_no_measures() {
    assert_eq!(broken_eval().1.tree.get(0).unwrap().utility_measures, [1.0]);
}

fn blocked_run(improver: &dyn Improver, start: State) -> Result<(), String> {
    let tmp = tempfile::tempdir().unwrap();
    let blocked = tmp.path().join("blocked");
    fs::create_dir_all(blocked.join("hgm_metadata.jsonl")).unwrap();
    let ok = |_: &str, _: &[String]| Ok(vec![1.0]);
    search_in(
        &Setup {
            dir: &blocked,
            tasks: &tasks(4),
            improver,
            log: &Log::discard(),
        },
        settings(2),
        start,
        &ok,
    )
    .0
}

#[test]
fn an_unrecordable_evaluation_is_an_error() {
    assert!(blocked_run(&Children::new(true), two_children()).is_err());
}

#[test]
fn an_unrecordable_expansion_is_an_error() {
    assert!(blocked_run(&Children::new(false), start_state(&[1.0])).is_err());
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
fn nothing_left_to_evaluate_spends_no_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let evaluate = |_: &str, _: &[String]| Ok(vec![1.0]);
    let mut start = state_with_all_submitted();
    start.n_task_evals = 1;
    start.tree.insert("y", Some(0), None);
    start
        .submitted_ids
        .insert(2, HashSet::from(["t0".to_owned()]));
    let (result, state) = search_in(
        &Setup {
            dir: tmp.path(),
            tasks: &tasks(1),
            improver: &Children::new(true),
            log: &Log::discard(),
        },
        settings(2),
        start,
        &evaluate,
    );
    result.unwrap();
    assert_eq!(state.n_task_evals, 1);
}

fn late_sample() -> (usize, State) {
    let tmp = tempfile::tempdir().unwrap();
    let calls = Mutex::new(0);
    let evaluate = |_: &str, _: &[String]| {
        *calls.lock().unwrap() += 1;
        Ok(vec![1.0])
    };
    let mut start = start_state(&[1.0]);
    start.tree.insert("x", Some(0), None);
    start.tree.insert("y", Some(0), None);
    let mut s = settings(1);
    s.jitter = Duration::from_millis(200);
    let (result, state) = search_in(
        &Setup {
            dir: tmp.path(),
            tasks: &tasks(4),
            improver: &Children::new(true),
            log: &Log::discard(),
        },
        s,
        start,
        &evaluate,
    );
    result.unwrap();
    (calls.into_inner().unwrap(), state)
}

#[test]
fn a_sample_that_wakes_after_the_budget_is_spent_does_nothing() {
    assert_eq!(late_sample().0, 1);
}

#[test]
fn a_late_sample_spends_no_more_budget() {
    assert_eq!(late_sample().1.n_task_evals, 1);
}
