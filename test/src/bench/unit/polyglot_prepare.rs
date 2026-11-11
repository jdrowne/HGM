use std::fs;

use rustyhgm_bench::polyglot_prepare::modify_cmake_file;

const MARKER: &str = "get_filename_component(exercise ${CMAKE_CURRENT_SOURCE_DIR} NAME)";

#[test]
fn the_first_exercise_lookup_is_replaced_with_the_task_name() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("CMakeLists.txt");
    fs::write(
        &path,
        format!("project(x)\n  \t{MARKER} # here\n{MARKER}\nend()"),
    )
    .unwrap();
    modify_cmake_file(&path, "two-fer").unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        format!("project(x)\n  \tset(exercise \"two-fer\")\n{MARKER}\nend()")
    );
}

#[test]
fn a_file_without_the_lookup_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("CMakeLists.txt");
    fs::write(&path, "project(x)\r\nend()").unwrap();
    modify_cmake_file(&path, "t").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "project(x)\r\nend()");
}

#[test]
fn a_missing_cmake_file_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(modify_cmake_file(&tmp.path().join("missing"), "t").is_err());
}
