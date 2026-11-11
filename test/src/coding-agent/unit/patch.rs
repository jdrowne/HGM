use hgm_coding_agent::patch::{
    filter_patch_by_files, remove_patch_by_files, remove_patches_to_tests, splitlines,
};
use serde_json::Value;

use crate::common::{cases, strings};

fn mismatches(prefix: &str, apply: impl Fn(&str, &Value) -> String) -> Vec<String> {
    cases("patch_filters")
        .iter()
        .filter(|case| case["name"].as_str().unwrap().starts_with(prefix))
        .filter_map(|case| {
            let got = apply(case["input"]["patch"].as_str().unwrap(), &case["input"]);
            (got != case["output"].as_str().unwrap()).then(|| format!("{}: {got:?}", case["name"]))
        })
        .collect()
}

#[test]
fn every_patch_filter_case_is_checked() {
    let unknown: Vec<String> = cases("patch_filters")
        .iter()
        .map(|case| case["name"].as_str().unwrap().to_owned())
        .filter(|name| {
            ![
                "filter_patch_by_files",
                "remove_patch_by_files",
                "remove_patches_to_tests",
            ]
            .iter()
            .any(|p| name.starts_with(p))
        })
        .collect();
    assert_eq!(unknown, Vec::<String>::new());
}

#[test]
fn filtering_a_patch_by_files_matches_the_python() {
    let found = mismatches("filter_patch_by_files", |patch, input| {
        filter_patch_by_files(patch, &strings(&input["target_files"]))
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn removing_a_patch_by_files_matches_the_python() {
    let found = mismatches("remove_patch_by_files", |patch, input| {
        remove_patch_by_files(patch, input["keyword"].as_str().unwrap())
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn removing_patches_to_tests_matches_the_python() {
    let found = mismatches("remove_patches_to_tests", |patch, _| {
        remove_patches_to_tests(patch)
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn splitlines_breaks_on_every_python_line_ending() {
    assert_eq!(splitlines("a\r\nb\rc\nd", false), ["a", "b", "c", "d"]);
}

#[test]
fn splitlines_can_keep_the_line_endings() {
    assert_eq!(splitlines("a\r\nb\n", true), ["a\r\n", "b\n"]);
}

#[test]
fn splitlines_of_nothing_is_empty() {
    assert_eq!(splitlines("", false), Vec::<&str>::new());
}

#[test]
fn splitlines_breaks_on_the_unicode_line_separator() {
    assert_eq!(splitlines("x\u{2028}y", false), ["x", "y"]);
}
