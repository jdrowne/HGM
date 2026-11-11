use hgm_agent::testrepo::{TestRepo, get_test_command, get_test_description};

use crate::common::cases;

#[test]
fn test_descriptions_match_the_python() {
    for case in cases("testrepo_prompt") {
        let name = case["name"].as_str().unwrap();
        let input = case["input"].as_str().unwrap();
        if name.starts_with("get_test_command_") {
            let (command, hint) = get_test_command(input);
            assert_eq!(serde_json::json!([command, hint]), case["output"], "{name}");
            continue;
        }
        let repo = if name.starts_with("get_test_description_swe_") {
            TestRepo::Swe
        } else if name == "get_test_description_polyglot" {
            TestRepo::Polyglot
        } else {
            TestRepo::Hgm
        };
        assert_eq!(
            get_test_description(input, repo),
            case["output"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn a_one_line_script_is_its_own_test_command() {
    let (command, _) = get_test_command("pytest -rA tests/test_a.py");
    assert_eq!(command, "cd /testbed/ && pytest -rA <specific test files>");
}
