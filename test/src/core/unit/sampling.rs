use rand::SeedableRng;
use rand::rngs::StdRng;
use rustyhgm_core::sampling::{CoolDown, argmax, beta_params, thompson_sample};
use serde_json::Value;

use crate::common::{cases, floats};

fn evals(v: &Value) -> Vec<Vec<f64>> {
    v.as_array().unwrap().iter().map(floats).collect()
}

fn close(actual: &[f64], expected: &[f64]) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a == e || ((a - e) / e).abs() < 1e-12)
}

fn ts_mismatches(
    check: impl Fn(&str, &Value, (Vec<f64>, Vec<f64>)) -> Option<String>,
) -> Vec<String> {
    cases("ts_sample")
        .iter()
        .filter_map(|case| {
            let input = &case["input"];
            let cool_down = input["cool_down"].as_bool().unwrap().then(|| CoolDown {
                beta: input["beta"].as_f64().unwrap(),
                max_task_evals: input["max_task_evals"].as_u64().unwrap() as usize,
                n_task_evals: input["n_task_evals"].as_u64().unwrap() as usize,
            });
            let params = beta_params(&evals(&input["evals"]), cool_down);
            check(case["name"].as_str().unwrap(), &case["output"], params)
        })
        .collect()
}

#[test]
fn beta_alphas_match_the_python() {
    let mismatches = ts_mismatches(|name, output, (alphas, _)| {
        let expected = floats(&output["alphas"]);
        (!close(&alphas, &expected)).then(|| format!("{name}: {alphas:?} != {expected:?}"))
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn beta_betas_match_the_python() {
    let mismatches = ts_mismatches(|name, output, (_, betas)| {
        let expected = floats(&output["betas"]);
        (!close(&betas, &expected)).then(|| format!("{name}: {betas:?} != {expected:?}"))
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn argmax_of_equal_thetas_matches_the_python() {
    let mismatches = ts_mismatches(|name, output, (alphas, _)| {
        let actual = argmax(&vec![1.0; alphas.len()]);
        let expected = output["argmax_when_all_thetas_equal"].as_u64().unwrap() as usize;
        (actual != Some(expected)).then(|| format!("{name}: {actual:?} != {expected}"))
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn argmax_takes_the_first_maximum() {
    assert_eq!(argmax(&[0.2, 0.9, 0.9, 0.1]), Some(1));
}

#[test]
fn argmax_of_nothing_is_nothing() {
    assert_eq!(argmax(&[]), None);
}

#[test]
fn sampling_nothing_selects_nothing() {
    let mut rng = StdRng::seed_from_u64(0);
    assert_eq!(thompson_sample(&[], None, &mut rng), None);
}

#[test]
fn sampling_favours_the_better_arm() {
    let mut rng = StdRng::seed_from_u64(7);
    let evals = vec![vec![0.0; 40], vec![1.0; 40], vec![0.0; 40]];
    let picks: Vec<usize> = (0..200)
        .map(|_| thompson_sample(&evals, None, &mut rng).unwrap())
        .collect();
    assert!(picks.iter().all(|&p| p == 1));
}

fn cool_down(n_task_evals: usize) -> CoolDown {
    CoolDown {
        beta: 1.0,
        max_task_evals: 10,
        n_task_evals,
    }
}

#[test]
fn cooling_down_at_the_budget_uses_the_fixed_factor() {
    assert_eq!(cool_down(10).factor(), 10000.0);
}

#[test]
fn cooling_down_past_the_budget_uses_the_fixed_factor() {
    assert_eq!(cool_down(12).factor(), 10000.0);
}

#[test]
fn cooling_down_before_the_budget_scales_with_the_evals_left() {
    assert_eq!(cool_down(5).factor(), 2.0);
}
