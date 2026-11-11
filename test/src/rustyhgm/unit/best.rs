use std::fs;
use std::path::{Path, PathBuf};

use rustyhgm::best::{
    Belief, beliefs, best_belief, check_epsilon, export, export_with, pick, quantile,
};
use serde_json::{Value, json};
use tempfile::TempDir;

fn node(commit: &str, mean: Value, evals: u64) -> Value {
    json!({"commit_id": commit, "id": 1, "parent_id": 0, "mean_utility": mean, "num_evals": evals})
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_quantile_of_a_flat_prior_at_one_percent_is_one_percent() {
    assert!(close(quantile(0, 0, 0.01), 0.01));
}

#[test]
fn the_quantile_of_a_flat_prior_at_half_is_half() {
    assert!(close(quantile(0, 0, 0.5), 0.5));
}

#[test]
fn the_quarter_quantile_after_one_success_is_half() {
    assert!(close(quantile(1, 1, 0.25), 0.5));
}

#[test]
fn the_three_quarter_quantile_after_one_failure_is_half() {
    assert!(close(quantile(0, 1, 0.75), 0.5));
}

#[test]
fn a_low_quantile_sits_below_the_observed_rate() {
    assert!(quantile(80, 100, 0.01) < 0.8);
}

#[test]
fn a_high_quantile_sits_above_the_observed_rate() {
    assert!(quantile(80, 100, 0.99) > 0.8);
}

fn streak_record() -> Value {
    json!({"n_task_evals": 103, "nodes": [
        node("lucky", json!(1.0), 3),
        node("steady", json!(0.8), 100),
        node("untested", json!("Infinity"), 0),
    ]})
}

#[test]
fn beliefs_count_successes_from_the_mean_utility() {
    let all = beliefs(&streak_record(), 0.01).unwrap();
    assert_eq!(
        all.iter()
            .map(|b| (b.commit_id.as_str(), b.successes, b.evals))
            .collect::<Vec<_>>(),
        [("lucky", 3, 3), ("steady", 80, 100), ("untested", 0, 0)]
    );
}

#[test]
fn evidence_outweighs_a_lucky_streak_at_a_low_quantile() {
    assert_eq!(
        best_belief(&streak_record(), 0.01).unwrap().commit_id,
        "steady"
    );
}

#[test]
fn a_lucky_streak_wins_at_a_high_quantile() {
    assert_eq!(
        best_belief(&streak_record(), 0.9).unwrap().commit_id,
        "lucky"
    );
}

fn tie_record() -> Value {
    json!({"nodes": [
        node("first", json!(0.5), 2),
        node("second", json!(0.5), 2),
        node("negative", json!(-1.0), 2),
        node("huge", json!(7.0), 2),
    ]})
}

#[test]
fn a_negative_mean_counts_as_no_successes() {
    assert_eq!(beliefs(&tie_record(), 0.5).unwrap()[2].successes, 0);
}

#[test]
fn a_mean_above_one_counts_as_no_successes() {
    assert_eq!(beliefs(&tie_record(), 0.5).unwrap()[3].successes, 0);
}

#[test]
fn ties_keep_the_earliest_agent() {
    let Belief {
        commit_id,
        successes,
        evals,
        ..
    } = best_belief(&tie_record(), 0.5).unwrap();
    assert_eq!((commit_id.as_str(), successes, evals), ("first", 1, 2));
}

#[test]
fn the_best_belief_carries_its_quantile() {
    let Belief { quantile, .. } = best_belief(&tie_record(), 0.5).unwrap();
    assert!(close(quantile, 0.5));
}

#[test]
fn epsilons_outside_the_open_unit_interval_are_errors() {
    let ok = json!({"nodes": [node("a", json!(0.0), 1)]});
    let rejected: Vec<bool> = [0.0, 1.0, -0.5, 2.0, f64::NAN]
        .into_iter()
        .map(|epsilon| {
            beliefs(&ok, epsilon)
                .unwrap_err()
                .starts_with("epsilon must be between 0 and 1")
        })
        .collect();
    assert_eq!(rejected, [true; 5]);
}

#[test]
fn a_record_without_nodes_is_an_error() {
    assert_eq!(
        beliefs(&json!({}), 0.5).unwrap_err(),
        "record has no nodes list"
    );
}

#[test]
fn a_node_without_a_commit_id_is_an_error() {
    assert_eq!(
        beliefs(&json!({"nodes": [{"num_evals": 1}]}), 0.5).unwrap_err(),
        "node has no commit_id"
    );
}

#[test]
fn a_node_without_num_evals_is_an_error() {
    assert_eq!(
        beliefs(&json!({"nodes": [{"commit_id": "c"}]}), 0.5).unwrap_err(),
        "node c has no num_evals"
    );
}

#[test]
fn a_record_listing_no_agents_has_no_best_belief() {
    assert_eq!(
        best_belief(&json!({"nodes": []}), 0.5).unwrap_err(),
        "the record lists no agents"
    );
}

#[test]
fn a_record_without_nodes_has_no_best_belief() {
    assert!(best_belief(&json!({}), 0.5).is_err());
}

fn two_record_run() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    fs::create_dir_all(&run).unwrap();
    let first = json!({"n_task_evals": 0, "nodes": []});
    let last = json!({"n_task_evals": 2, "nodes": [node("c", json!(1.0), 2)]});
    fs::write(
        run.join("rustyhgm_metadata.jsonl"),
        format!("{first:#}\n{last:#}\n"),
    )
    .unwrap();
    (tmp, run)
}

#[test]
fn pick_reads_the_last_record_of_a_run() {
    let (_tmp, run) = two_record_run();
    assert_eq!(pick(&run, 0.01).unwrap().commit_id, "c");
}

#[test]
fn pick_rejects_a_bad_epsilon() {
    let (_tmp, run) = two_record_run();
    assert!(pick(&run, 0.0).is_err());
}

#[test]
fn pick_on_a_missing_run_names_the_metadata_file() {
    let (tmp, _run) = two_record_run();
    assert!(
        pick(&tmp.path().join("missing"), 0.01)
            .unwrap_err()
            .contains("rustyhgm_metadata.jsonl")
    );
}

#[test]
fn check_epsilon_accepts_a_half() {
    assert!(check_epsilon(0.5).is_ok());
}

#[test]
fn check_epsilon_rejects_one() {
    assert!(check_epsilon(1.0).is_err());
}

fn version(run: &Path, commit: &str, parent: &str, patch: Option<&str>) {
    let dir = run.join(commit);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("metadata.json"),
        json!({"parent_commit": parent}).to_string(),
    )
    .unwrap();
    if let Some(patch) = patch {
        fs::write(dir.join("model_patch.diff"), patch).unwrap();
    }
}

const FIRST: &str = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n seed\n+one\n";
const SECOND: &str = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1,2 +1,3 @@\n seed\n one\n+two\n";

fn exported_chain() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    let seed = run.join("initial/src");
    fs::create_dir_all(seed.join("sub")).unwrap();
    fs::write(seed.join("lib.rs"), "seed\n").unwrap();
    fs::write(seed.join("sub/keep.txt"), "k").unwrap();
    version(&run, "c1", "initial", Some(FIRST));
    version(&run, "c2", "c1", Some(SECOND));
    let dest = run.join("best_agent");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("stale"), "").unwrap();
    export(&run, "c2", &dest).unwrap();
    (tmp, dest)
}

#[test]
fn export_replays_the_patch_chain_onto_the_seed() {
    let (_tmp, dest) = exported_chain();
    assert_eq!(
        fs::read_to_string(dest.join("lib.rs")).unwrap(),
        "seed\none\ntwo\n"
    );
}

#[test]
fn export_keeps_untouched_seed_files() {
    let (_tmp, dest) = exported_chain();
    assert_eq!(fs::read_to_string(dest.join("sub/keep.txt")).unwrap(), "k");
}

#[test]
fn export_clears_stale_files_from_the_destination() {
    let (_tmp, dest) = exported_chain();
    assert!(!dest.join("stale").exists());
}

#[test]
fn export_leaves_no_git_directory() {
    let (_tmp, dest) = exported_chain();
    assert!(!dest.join(".git").exists());
}

#[test]
fn export_of_the_initial_agent_over_an_earlier_export_is_the_seed() {
    let (tmp, dest) = exported_chain();
    export(&tmp.path().join("run"), "initial", &dest).unwrap();
    assert_eq!(fs::read_to_string(dest.join("lib.rs")).unwrap(), "seed\n");
}

#[test]
fn export_fails_without_a_seed() {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    assert!(
        export(&run, "initial", &run.join("best_agent"))
            .unwrap_err()
            .contains("the seed source is missing")
    );
}

fn mismatched_seed_run() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    fs::create_dir_all(run.join("initial/src")).unwrap();
    fs::write(run.join("initial/src/lib.rs"), "other\n").unwrap();
    version(&run, "c1", "initial", Some(FIRST));
    (tmp, run)
}

#[test]
fn export_fails_on_a_patch_that_does_not_apply() {
    let (_tmp, run) = mismatched_seed_run();
    assert!(
        export(&run, "c1", &run.join("best_agent"))
            .unwrap_err()
            .contains("git apply")
    );
}

#[test]
fn export_fails_for_an_unknown_commit() {
    let (_tmp, run) = mismatched_seed_run();
    assert!(export(&run, "missing", &run.join("best_agent")).is_err());
}

fn chain_run() -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    fs::create_dir_all(run.join("initial/src")).unwrap();
    fs::write(run.join("initial/src/lib.rs"), "seed\n").unwrap();
    version(&run, "c1", "initial", Some(FIRST));
    (tmp, run)
}

#[test]
fn export_fails_without_the_git_program() {
    let (_tmp, run) = chain_run();
    assert!(
        export_with("rustyhgm-no-such-git", &run, "c1", &run.join("best_agent"))
            .unwrap_err()
            .contains("rustyhgm-no-such-git apply")
    );
}

#[test]
fn export_fails_for_an_empty_run_folder() {
    assert!(export(Path::new(""), "c1", Path::new("best_agent")).is_err());
}

#[test]
fn export_fails_for_an_empty_destination() {
    let (_tmp, run) = chain_run();
    assert!(export(&run, "c1", Path::new("")).is_err());
}

#[test]
fn export_fails_when_the_destination_is_a_file() {
    let (_tmp, run) = chain_run();
    let dest = run.join("best_agent");
    fs::write(&dest, "").unwrap();
    assert!(export(&run, "c1", &dest).is_err());
}

#[test]
fn export_fails_when_the_destination_is_under_a_file() {
    let (_tmp, run) = chain_run();
    let file = run.join("file");
    fs::write(&file, "").unwrap();
    assert!(export(&run, "c1", &file.join("best_agent")).is_err());
}

#[test]
fn export_ignores_a_repository_around_the_destination() {
    let (_tmp, run) = chain_run();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&run)
        .status()
        .unwrap();
    let dest = run.join("best_agent");
    export(&run, "c1", &dest).unwrap();
    assert_eq!(
        fs::read_to_string(dest.join("lib.rs")).unwrap(),
        "seed\none\n"
    );
}
