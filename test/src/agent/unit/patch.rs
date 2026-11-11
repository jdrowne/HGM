use hgm_agent::patch::{
    filter_patch_by_files, remove_patch_by_files, remove_patches_to_tests, splitlines,
};

use crate::common::{cases, strings};

#[test]
fn patch_filters_match_the_python() {
    for case in cases("patch_filters") {
        let name = case["name"].as_str().unwrap();
        let input = &case["input"];
        let patch = input["patch"].as_str().unwrap();
        let got = if name.starts_with("filter_patch_by_files") {
            filter_patch_by_files(patch, &strings(&input["target_files"]))
        } else if name.starts_with("remove_patch_by_files") {
            remove_patch_by_files(patch, input["keyword"].as_str().unwrap())
        } else {
            remove_patches_to_tests(patch)
        };
        assert_eq!(got, case["output"].as_str().unwrap(), "{name}");
    }
}

#[test]
fn splitlines_follows_python() {
    assert_eq!(splitlines("a\r\nb\rc\nd", false), ["a", "b", "c", "d"]);
    assert_eq!(splitlines("a\r\nb\n", true), ["a\r\n", "b\n"]);
    assert_eq!(splitlines("", false), Vec::<&str>::new());
    assert_eq!(splitlines("x\u{2028}y", false), ["x", "y"]);
}
