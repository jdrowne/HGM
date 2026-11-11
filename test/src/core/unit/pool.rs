use std::sync::atomic::{AtomicUsize, Ordering};

use hgm_core::pool::map;

#[test]
fn results_keep_the_input_order() {
    let items: Vec<u64> = (0..50).collect();
    assert_eq!(
        map(&items, 4, |n| n * 2),
        items.iter().map(|n| n * 2).collect::<Vec<_>>()
    );
}

#[test]
fn every_item_runs_once_whatever_the_worker_count() {
    for workers in [0, 1, 3, 100] {
        let calls = AtomicUsize::new(0);
        let out = map(&[1, 2, 3], workers, |n| {
            calls.fetch_add(1, Ordering::SeqCst);
            *n
        });
        assert_eq!(out, [1, 2, 3]);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
    let empty: [u8; 0] = [];
    assert!(map(&empty, 2, |n| *n).is_empty());
}
