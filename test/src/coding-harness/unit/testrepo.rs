use rustyhgm_coding_harness::testrepo::{TestRepo, get_test_command, get_test_description};
use serde_json::{Value, json};

use crate::common::cases;

fn mismatches(keep: impl Fn(&str) -> bool, actual: impl Fn(&str, &str) -> Value) -> Vec<String> {
    cases("testrepo_prompt")
        .iter()
        .filter(|case| keep(case["name"].as_str().unwrap()))
        .filter_map(|case| {
            let name = case["name"].as_str().unwrap();
            let got = actual(name, case["input"].as_str().unwrap());
            (got != case["output"]).then(|| format!("{name}: {got}"))
        })
        .collect()
}

fn is_command(name: &str) -> bool {
    name.starts_with("get_test_command_")
}

#[test]
fn test_commands_match_the_python() {
    let found = mismatches(is_command, |_, input| {
        let (command, hint) = get_test_command(input);
        json!([command, hint])
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn test_descriptions_match_the_python() {
    let found = mismatches(
        |name| !is_command(name),
        |name, input| {
            let repo = if name.starts_with("get_test_description_swe_") {
                TestRepo::Swe
            } else {
                TestRepo::Hgm
            };
            json!(get_test_description(input, repo))
        },
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_polyglot_description_says_where_the_tests_are_and_how_to_run_them() {
    assert_eq!(
        get_test_description("cd build\nmake", TestRepo::Polyglot),
        "The repository holds the tests that decide whether the problem is solved. Changes to the test files are discarded before scoring. In the repository folder, the tests can be run with the following bash command(s):\n\n```\ncd build\nmake\n```"
    );
}

#[test]
fn a_one_line_script_is_its_own_test_command() {
    let (command, _) = get_test_command("pytest -rA tests/test_a.py");
    assert_eq!(command, "cd /testbed/ && pytest -rA <specific test files>");
}
