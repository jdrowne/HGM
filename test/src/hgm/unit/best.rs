use std::fs;

use hgm::best::{Belief, beliefs, best, best_belief, quantile};
use hgm::cli::BestArgs;
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
fn best_reads_the_last_record_of_a_run() {
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
    let args = |dir: &str, epsilon: f64| BestArgs {
        run_dir: dir.into(),
        epsilon,
    };
    best(&args("run", 0.01), tmp.path()).unwrap();
    assert!(best(&args("run", 0.0), tmp.path()).is_err());
    assert!(
        best(&args("missing", 0.01), tmp.path())
            .unwrap_err()
            .contains("hgm_metadata.jsonl")
    );
}
