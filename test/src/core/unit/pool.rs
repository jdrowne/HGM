use std::sync::atomic::{AtomicUsize, Ordering};

use rustyhgm_core::pool::map;

const WORKER_COUNTS: [usize; 4] = [0, 1, 3, 100];

fn run_with(workers: usize) -> (Vec<i32>, usize) {
    let calls = AtomicUsize::new(0);
    let out = map(&[1, 2, 3], workers, |n| {
        calls.fetch_add(1, Ordering::SeqCst);
        *n
    });
    (out, calls.load(Ordering::SeqCst))
}

#[test]
fn results_keep_the_input_order() {
    let items: Vec<u64> = (0..50).collect();
    assert_eq!(
        map(&items, 4, |n| n * 2),
        items.iter().map(|n| n * 2).collect::<Vec<_>>()
    );
}

#[test]
fn every_worker_count_returns_every_result() {
    let outs: Vec<Vec<i32>> = WORKER_COUNTS.iter().map(|w| run_with(*w).0).collect();
    assert_eq!(outs, vec![vec![1, 2, 3]; WORKER_COUNTS.len()]);
}

#[test]
fn every_worker_count_runs_each_item_once() {
    let calls: Vec<usize> = WORKER_COUNTS.iter().map(|w| run_with(*w).1).collect();
    assert_eq!(calls, vec![3; WORKER_COUNTS.len()]);
}

#[test]
fn mapping_nothing_returns_nothing() {
    let empty: [u8; 0] = [];
    assert!(map(&empty, 2, |n| *n).is_empty());
}
