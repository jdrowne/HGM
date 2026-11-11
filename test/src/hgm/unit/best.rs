use std::fs;
use std::path::Path;

use hgm::best::{Belief, beliefs, best_belief, check_epsilon, export, pick, quantile};
use serde_json::{Value, json};

fn node(commit: &str, mean: Value, evals: u64) -> Value {
    json!({"commit_id": commit, "id": 1, "parent_id": 0, "mean_utility": mean, "num_evals": evals})
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_quantile_is_taken_from_the_beta_posterior() {
    assert!(close(quantile(0, 0, 0.01), 0.01));
    assert!(close(quantile(0, 0, 0.5), 0.5));
    assert!(close(quantile(1, 1, 0.25), 0.5));
    assert!(close(quantile(0, 1, 0.75), 0.5));
    assert!(quantile(80, 100, 0.01) < 0.8);
    assert!(quantile(80, 100, 0.99) > 0.8);
}

#[test]
fn evidence_outweighs_a_lucky_streak_at_a_low_quantile() {
    let record = json!({"n_task_evals": 103, "nodes": [
        node("lucky", json!(1.0), 3),
        node("steady", json!(0.8), 100),
        node("untested", json!("Infinity"), 0),
    ]});
    let all = beliefs(&record, 0.01).unwrap();
    assert_eq!(
        all.iter()
            .map(|b| (b.commit_id.as_str(), b.successes, b.evals))
            .collect::<Vec<_>>(),
        [("lucky", 3, 3), ("steady", 80, 100), ("untested", 0, 0)]
    );
    assert_eq!(best_belief(&record, 0.01).unwrap().commit_id, "steady");
    assert_eq!(best_belief(&record, 0.9).unwrap().commit_id, "lucky");
}

#[test]
fn ties_keep_the_earliest_agent_and_odd_means_count_as_no_successes() {
    let record = json!({"nodes": [
        node("first", json!(0.5), 2),
        node("second", json!(0.5), 2),
        node("negative", json!(-1.0), 2),
        node("huge", json!(7.0), 2),
    ]});
    let all = beliefs(&record, 0.5).unwrap();
    assert_eq!(all[2].successes, 0);
    assert_eq!(all[3].successes, 0);
    let Belief {
        commit_id,
        successes,
        evals,
        quantile,
    } = best_belief(&record, 0.5).unwrap();
    assert_eq!((commit_id.as_str(), successes, evals), ("first", 1, 2));
    assert!(close(quantile, 0.5));
}

#[test]
fn bad_records_and_epsilons_are_errors() {
    let ok = json!({"nodes": [node("a", json!(0.0), 1)]});
    for epsilon in [0.0, 1.0, -0.5, 2.0, f64::NAN] {
        assert!(
            beliefs(&ok, epsilon)
                .unwrap_err()
                .starts_with("epsilon must be between 0 and 1"),
            "{epsilon}"
        );
    }
    assert_eq!(
        beliefs(&json!({}), 0.5).unwrap_err(),
        "record has no nodes list"
    );
    assert_eq!(
        beliefs(&json!({"nodes": [{"num_evals": 1}]}), 0.5).unwrap_err(),
        "node has no commit_id"
    );
    assert_eq!(
        beliefs(&json!({"nodes": [{"commit_id": "c"}]}), 0.5).unwrap_err(),
        "node c has no num_evals"
    );
    assert_eq!(
        best_belief(&json!({"nodes": []}), 0.5).unwrap_err(),
        "the record lists no agents"
    );
    assert!(best_belief(&json!({}), 0.5).is_err());
}

#[test]
fn pick_reads_the_last_record_of_a_run() {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    fs::create_dir_all(&run).unwrap();
    let first = json!({"n_task_evals": 0, "nodes": []});
    let last = json!({"n_task_evals": 2, "nodes": [node("c", json!(1.0), 2)]});
    fs::write(
        run.join("hgm_metadata.jsonl"),
        format!("{first:#}\n{last:#}\n"),
    )
    .unwrap();
    assert_eq!(pick(&run, 0.01).unwrap().commit_id, "c");
    assert!(pick(&run, 0.0).is_err());
    assert!(
        pick(&tmp.path().join("missing"), 0.01)
            .unwrap_err()
            .contains("hgm_metadata.jsonl")
    );
    assert!(check_epsilon(0.5).is_ok());
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

#[test]
fn export_replays_the_patch_chain_onto_the_seed() {
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
    assert_eq!(
        fs::read_to_string(dest.join("lib.rs")).unwrap(),
        "seed\none\ntwo\n"
    );
    assert_eq!(fs::read_to_string(dest.join("sub/keep.txt")).unwrap(), "k");
    assert!(!dest.join("stale").exists());
    assert!(!dest.join(".git").exists());
    export(&run, "initial", &dest).unwrap();
    assert_eq!(fs::read_to_string(dest.join("lib.rs")).unwrap(), "seed\n");
}

#[test]
fn export_fails_without_a_seed_or_on_a_patch_that_does_not_apply() {
    let tmp = tempfile::tempdir().unwrap();
    let run = tmp.path().join("run");
    let dest = run.join("best_agent");
    assert!(
        export(&run, "initial", &dest)
            .unwrap_err()
            .contains("the seed source is missing")
    );
    fs::create_dir_all(run.join("initial/src")).unwrap();
    fs::write(run.join("initial/src/lib.rs"), "other\n").unwrap();
    version(&run, "c1", "initial", Some(FIRST));
    assert!(export(&run, "c1", &dest).unwrap_err().contains("git apply"));
    assert!(export(&run, "missing", &dest).is_err());
}
