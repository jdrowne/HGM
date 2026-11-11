use hgm_bench::pyrandom::PyRandom;

use crate::common::cases;

#[test]
fn shuffle_matches_cpython() {
    for case in cases("py_shuffle") {
        let seed = case["input"]["seed"].as_u64().unwrap();
        let n = case["input"]["n"].as_u64().unwrap();
        let mut items: Vec<u64> = (0..n).collect();
        PyRandom::seed(seed).shuffle(&mut items);
        let expected: Vec<u64> = case["output"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(items, expected, "{}", case["name"]);
    }
}
