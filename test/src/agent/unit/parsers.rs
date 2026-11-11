use hgm_agent::parsers::{parser, parser_for_repo, parser_name};
use serde_json::{Value, json};

use crate::common::cases;

#[test]
fn repos_map_to_the_python_parsers() {
    let map = cases("swe_log_parsers")
        .into_iter()
        .find(|c| c["name"] == "map_repo_to_parser")
        .unwrap();
    let mismatches: Vec<String> = map["output"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(repo, want)| parser_name(repo) != want.as_str())
        .map(|(repo, _)| repo.clone())
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn log_parsers_match_the_python() {
    let mismatches: Vec<String> = cases("swe_log_parsers")
        .iter()
        .filter(|c| c["name"] != "map_repo_to_parser")
        .filter_map(|c| {
            let parse = parser(c["input"]["parser"].as_str().unwrap()).unwrap();
            let want = &c["output"];
            let ok = match parse(c["input"]["log"].as_str().unwrap()) {
                Some(report) => Value::Object(report) == want["ok"],
                None => want["error"].is_string(),
            };
            (!ok).then(|| c["name"].as_str().unwrap().to_owned())
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

fn parse(name: &str, log: &str) -> Option<Value> {
    parser(name).unwrap()(log).map(Value::Object)
}

#[test]
fn an_unknown_parser_is_none() {
    assert!(parser("parse_log_nope").is_none());
}

#[test]
fn an_unknown_repo_has_no_parser_name() {
    assert!(parser_name("me/mine").is_none());
}

#[test]
fn an_unknown_repo_has_no_parser() {
    assert!(parser_for_repo("me/mine").is_none());
}

#[test]
fn the_hgm_repo_has_a_parser() {
    assert!(parser_for_repo("hgm").is_some());
}

#[test]
fn a_short_django_fail_line_crashes_its_parser() {
    assert_eq!(parse("parse_log_django", "FAIL:"), None);
}

#[test]
fn a_short_django_error_line_crashes_its_parser() {
    assert_eq!(parse("parse_log_django", "ERROR:"), None);
}

#[test]
fn a_bare_seaborn_failed_crashes_its_parser() {
    assert_eq!(parse("parse_log_seaborn", "FAILED"), None);
}

#[test]
fn a_bare_seaborn_passed_crashes_its_parser() {
    assert_eq!(parse("parse_log_seaborn", "PASSED"), None);
}

#[test]
fn a_padded_seaborn_passed_crashes_its_parser() {
    assert_eq!(parse("parse_log_seaborn", " PASSED "), None);
}

#[test]
fn a_bare_pytest_v2_xfailed_crashes_its_parser() {
    assert_eq!(parse("parse_log_pytest_v2", "xFAILED"), None);
}

#[test]
fn a_bare_pytest_passed_reports_nothing() {
    assert_eq!(parse("parse_log_pytest", "PASSED"), Some(json!({})));
}

#[test]
fn a_seaborn_passed_mid_line_reports_nothing() {
    assert_eq!(parse("parse_log_seaborn", "a b PASSED c"), Some(json!({})));
}

#[test]
fn pytest_options_keep_the_last_path_segment() {
    assert_eq!(
        parse("parse_log_pytest_options", "PASSED t[/a/b]\nPASSED"),
        Some(json!({"t[/b]": "PASSED"}))
    );
}

fn django_report() -> Value {
    let log = "Applying sites.0002_alter_domain_unique...test_no_migrations (x.Tests) ... ok\n--version is equivalent to version\nt1 (a.B) ... ERROR\nt2 (a.B) ... slow output\nok\nt3 (a.B) ... Internal Server Error: /x/\nok";
    parse("parse_log_django", log).unwrap()
}

#[test]
fn django_reads_a_test_after_a_migration_line() {
    assert_eq!(django_report()["test_no_migrations (x.Tests)"], "PASSED");
}

#[test]
fn django_reads_the_version_line_as_passed() {
    assert_eq!(
        django_report()["--version is equivalent to version"],
        "PASSED"
    );
}

#[test]
fn django_reads_an_error() {
    assert_eq!(django_report()["t1 (a.B)"], "ERROR");
}

#[test]
fn django_reads_an_ok_after_slow_output() {
    assert_eq!(django_report()["t2 (a.B)"], "PASSED");
}

#[test]
fn django_reads_an_ok_after_a_server_error() {
    assert_eq!(django_report()["t3 (a.B)"], "PASSED");
}
