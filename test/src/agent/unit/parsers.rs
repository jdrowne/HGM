use hgm_agent::parsers::{parser, parser_for_repo, parser_name};
use serde_json::{Value, json};

use crate::common::cases;

#[test]
fn log_parsers_match_the_python() {
    for c in cases("swe_log_parsers") {
        let name = c["name"].as_str().unwrap();
        if name == "map_repo_to_parser" {
            for (repo, parser) in c["output"].as_object().unwrap() {
                assert_eq!(parser_name(repo), parser.as_str(), "{repo}");
            }
            continue;
        }
        let parse = parser(c["input"]["parser"].as_str().unwrap()).unwrap();
        let got = parse(c["input"]["log"].as_str().unwrap());
        let want = &c["output"];
        match got {
            Some(report) => assert_eq!(Value::Object(report), want["ok"], "{name}"),
            None => assert!(want["error"].is_string(), "{name}"),
        }
    }
}

fn parse(name: &str, log: &str) -> Option<Value> {
    parser(name).unwrap()(log).map(Value::Object)
}

#[test]
fn unknown_parsers_and_repos_have_none() {
    assert!(parser("parse_log_nope").is_none());
    assert!(parser_name("me/mine").is_none());
    assert!(parser_for_repo("me/mine").is_none());
    assert!(parser_for_repo("hgm").is_some());
}

#[test]
fn short_lines_crash_the_parsers_that_index_them() {
    assert_eq!(parse("parse_log_django", "FAIL:"), None);
    assert_eq!(parse("parse_log_django", "ERROR:"), None);
    assert_eq!(parse("parse_log_seaborn", "FAILED"), None);
    assert_eq!(parse("parse_log_seaborn", "PASSED"), None);
    assert_eq!(parse("parse_log_seaborn", " PASSED "), None);
    assert_eq!(parse("parse_log_pytest_v2", "xFAILED"), None);
    assert_eq!(parse("parse_log_pytest", "PASSED"), Some(json!({})));
    assert_eq!(parse("parse_log_seaborn", "a b PASSED c"), Some(json!({})));
    assert_eq!(
        parse("parse_log_pytest_options", "PASSED t[/a/b]\nPASSED"),
        Some(json!({"t[/b]": "PASSED"}))
    );
}

#[test]
fn django_handles_its_odd_layouts() {
    let log = "Applying sites.0002_alter_domain_unique...test_no_migrations (x.Tests) ... ok\n--version is equivalent to version\nt1 (a.B) ... ERROR\nt2 (a.B) ... slow output\nok\nt3 (a.B) ... Internal Server Error: /x/\nok";
    let report = parse("parse_log_django", log).unwrap();
    assert_eq!(report["test_no_migrations (x.Tests)"], "PASSED");
    assert_eq!(report["--version is equivalent to version"], "PASSED");
    assert_eq!(report["t1 (a.B)"], "ERROR");
    assert_eq!(report["t2 (a.B)"], "PASSED");
    assert_eq!(report["t3 (a.B)"], "PASSED");
}
