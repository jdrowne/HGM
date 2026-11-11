use hgm_core::sampling::{CoolDown, argmax, beta_params, thompson_sample};
use rand::SeedableRng;
use rand::rngs::StdRng;
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

#[test]
fn beta_parameters_match_the_python() {
    for case in cases("ts_sample") {
        let input = &case["input"];
        let cool_down = input["cool_down"].as_bool().unwrap().then(|| CoolDown {
            beta: input["beta"].as_f64().unwrap(),
            max_task_evals: input["max_task_evals"].as_u64().unwrap() as usize,
            n_task_evals: input["n_task_evals"].as_u64().unwrap() as usize,
        });
        let (alphas, betas) = beta_params(&evals(&input["evals"]), cool_down);
        let name = case["name"].as_str().unwrap();
        let expected_alphas = floats(&case["output"]["alphas"]);
        let expected_betas = floats(&case["output"]["betas"]);
        assert!(
            close(&alphas, &expected_alphas),
            "{name}: {alphas:?} != {expected_alphas:?}"
        );
        assert!(
            close(&betas, &expected_betas),
            "{name}: {betas:?} != {expected_betas:?}"
        );
        let ties = vec![1.0; alphas.len()];
        assert_eq!(
            argmax(&ties),
            Some(
                case["output"]["argmax_when_all_thetas_equal"]
                    .as_u64()
                    .unwrap() as usize
            ),
            "{name}"
        );
    }
}

#[test]
fn argmax_takes_the_first_maximum() {
    assert_eq!(argmax(&[0.2, 0.9, 0.9, 0.1]), Some(1));
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

#[test]
fn cooling_down_at_the_budget_uses_the_fixed_factor() {
    let at = CoolDown {
        beta: 1.0,
        max_task_evals: 10,
        n_task_evals: 10,
    };
    let past = CoolDown {
        n_task_evals: 12,
        ..at
    };
    assert_eq!(at.factor(), 10000.0);
    assert_eq!(past.factor(), 10000.0);
    assert_eq!(
        CoolDown {
            n_task_evals: 5,
            ..at
        }
        .factor(),
        2.0
    );
}
