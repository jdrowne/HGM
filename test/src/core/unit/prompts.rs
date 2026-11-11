use std::fs;
use std::path::{Path, PathBuf};

use hgm_core::prompts::{
    self, DiagnoseInput, EvalLogs, any_exceeding_context_length, format_named,
    get_diagnose_prompt_polyglot, get_diagnose_prompt_swe, get_eval_log_text,
    get_problem_description_prompt, process_selfimprove_eval_logs, read_mdlog_file, relpath,
};
use serde_json::{Value, json};

use crate::common::{cases, fixtures_dir};

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn templates() -> [(&'static str, &'static str); 12] {
    [
        ("coding_agent_summary", prompts::CODING_AGENT_SUMMARY),
        (
            "coding_agent_summary_polyglot",
            prompts::CODING_AGENT_SUMMARY_POLYGLOT,
        ),
        ("diagnose_system_message", prompts::DIAGNOSE_SYSTEM_MESSAGE),
        ("swe_issue_prompt", prompts::SWE_ISSUE_PROMPT),
        ("polyglot_issue_prompt", prompts::POLYGLOT_ISSUE_PROMPT),
        ("diagnose_prompt", prompts::DIAGNOSE_PROMPT),
        (
            "diagnose_prompt_emptypatches",
            prompts::DIAGNOSE_PROMPT_EMPTYPATCHES,
        ),
        (
            "diagnose_prompt_stochasticity",
            prompts::DIAGNOSE_PROMPT_STOCHASTICITY,
        ),
        (
            "diagnose_prompt_contextlength",
            prompts::DIAGNOSE_PROMPT_CONTEXTLENGTH,
        ),
        (
            "problem_description_prompt",
            prompts::PROBLEM_DESCRIPTION_PROMPT,
        ),
        (
            "diagnose_prompt_emptypatches_polyglot",
            prompts::DIAGNOSE_PROMPT_EMPTYPATCHES_POLYGLOT,
        ),
        (
            "diagnose_prompt_stochasticity_polyglot",
            prompts::DIAGNOSE_PROMPT_STOCHASTICITY_POLYGLOT,
        ),
    ]
}

#[test]
fn every_template_on_disk_is_checked() {
    let on_disk = fs::read_dir(fixtures_dir().join("prompts"))
        .unwrap()
        .count();
    assert_eq!(on_disk, templates().len());
}

#[test]
fn templates_match_the_python() {
    let dir = fixtures_dir().join("prompts");
    let mismatches: Vec<&str> = templates()
        .into_iter()
        .filter(|(name, text)| {
            *text != fs::read_to_string(dir.join(format!("{name}.txt"))).unwrap()
        })
        .map(|(name, _)| name)
        .collect();
    assert_eq!(mismatches, Vec::<&str>::new());
}

const HELPER_PREFIXES: [&str; 4] = [
    "get_problem_description_prompt",
    "process_selfimprove_eval_logs",
    "get_eval_log_text",
    "read_mdlog_file",
];

fn helper_mismatches(prefix: &str, actual: impl Fn(&str, &Value) -> Value) -> Vec<String> {
    cases("self_improvement_prompt")
        .iter()
        .filter(|case| case["name"].as_str().unwrap().starts_with(prefix))
        .filter_map(|case| {
            let name = case["name"].as_str().unwrap();
            let got = actual(name, &case["input"]);
            (got != case["output"]).then(|| format!("{name}: {got}"))
        })
        .collect()
}

#[test]
fn every_helper_case_is_checked() {
    let unhandled: Vec<String> = cases("self_improvement_prompt")
        .iter()
        .map(|case| case["name"].as_str().unwrap().to_owned())
        .filter(|name| !HELPER_PREFIXES.iter().any(|p| name.starts_with(p)))
        .collect();
    assert_eq!(unhandled, Vec::<String>::new());
}

#[test]
fn problem_description_prompts_match_the_python() {
    let mismatches = helper_mismatches(HELPER_PREFIXES[0], |_, input| {
        json!(
            get_problem_description_prompt(
                &input["response_json"],
                input["is_polyglot"].as_bool().unwrap(),
            )
            .unwrap()
        )
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

fn tail(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars[chars.len().saturating_sub(n)..].iter().collect()
}

#[test]
fn processed_eval_logs_match_the_python() {
    let mismatches = helper_mismatches(HELPER_PREFIXES[1], |name, input| {
        let args: Vec<Vec<String>> = input.as_array().unwrap().iter().map(strings).collect();
        let clipped = name.ends_with("clipped");
        let logs = EvalLogs {
            md_logs: if clipped {
                vec!["x".repeat(100_005)]
            } else {
                args[0].clone()
            },
            eval_logs: args[1].clone(),
            predicted_patches: args[2].clone(),
            eval_results: args[3].clone(),
        };
        let got = process_selfimprove_eval_logs(&logs);
        let shown = if clipped {
            json!({"len": got.md_log.chars().count(), "tail": tail(&got.md_log, 20)})
        } else {
            json!(got.md_log)
        };
        json!([shown, got.eval_log, got.predicted_patch, got.eval_result])
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn eval_log_texts_match_the_python() {
    let mismatches = helper_mismatches(HELPER_PREFIXES[2], |_, input| {
        json!(get_eval_log_text(input))
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn read_md_logs_match_the_python() {
    let tmp = tempfile::tempdir().unwrap();
    let mismatches = helper_mismatches(HELPER_PREFIXES[3], |_, input| {
        let path = tmp.path().join("log.md");
        fs::write(&path, input["text"].as_str().unwrap()).unwrap();
        json!(read_mdlog_file(&path, input["filter"].as_bool().unwrap()).unwrap())
    });
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn context_length_matches_the_python() {
    let tmp = tempfile::tempdir().unwrap();
    let mismatches: Vec<String> = cases("context_length")
        .iter()
        .filter_map(|case| {
            let name = case["name"].as_str().unwrap();
            let pred = tmp.path().join(name).join("predictions").join("run0");
            fs::create_dir_all(&pred).unwrap();
            fs::write(
                pred.join("inst-1.md"),
                case["input"]["md_log"].as_str().unwrap(),
            )
            .unwrap();
            let got = json!(any_exceeding_context_length(tmp.path(), name, &["inst-1"]).unwrap());
            (got != case["output"]).then(|| format!("{name}: {got}"))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn format_named_substitutes_once() {
    assert_eq!(
        format_named("{a} and {b} {c}", &[("a", "{b}"), ("b", "B")]),
        "{b} and B {c}"
    );
}

#[test]
fn relpath_walks_down() {
    let root = PathBuf::from("/x/y");
    assert_eq!(relpath(&root.join("a/b.py"), &root), "a/b.py");
}

#[test]
fn relpath_walks_up() {
    assert_eq!(
        relpath(&PathBuf::from("/x/z/p.diff"), &PathBuf::from("/x/y")),
        "../z/p.diff"
    );
}

#[test]
fn relpath_of_the_root_is_dot() {
    let root = PathBuf::from("/x/y");
    assert_eq!(relpath(&root, &root), ".");
}

fn rng_drawing(below: bool) -> rand::rngs::StdRng {
    use rand::{RngExt, SeedableRng};
    let seed = (0..)
        .find(|s| (rand::rngs::StdRng::seed_from_u64(*s).random::<f64>() < 0.25) == below)
        .unwrap();
    rand::rngs::StdRng::seed_from_u64(seed)
}

#[test]
fn diagnose_prompts_match_the_python() {
    let all = cases("diagnose_prompts");
    let tmp = tempfile::tempdir().unwrap();
    for (rel, text) in all[0]["input"].as_object().unwrap() {
        let path = tmp.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text.as_str().unwrap()).unwrap();
    }
    let dataset = all[0]["output"].as_array().unwrap().clone();
    let root = tmp.path().join("root");
    let out = tmp.path().join("out");
    let patches = [out.join("c1/model_patch.diff")];
    let mismatches: Vec<String> = all[1..]
        .iter()
        .filter_map(|case| {
            let name = case["name"].as_str().unwrap();
            let input = DiagnoseInput {
                entry_id: case["input"]["entry_id"].as_str().unwrap(),
                commit: "c1",
                root_dir: &root,
                out_dir: &out,
                dataset: &dataset,
                patch_files: if case["input"]["patch_files"] == true {
                    &patches
                } else {
                    &[]
                },
            };
            let got = match case["input"]["draw"].as_f64() {
                None => get_diagnose_prompt_swe(&input).unwrap(),
                Some(draw) => {
                    get_diagnose_prompt_polyglot(&input, &mut rng_drawing(draw < 0.25)).unwrap()
                }
            };
            let got = json!([got.0, got.1]);
            (got != case["output"]).then(|| format!("{name}: {got}"))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

fn absent_input<'a>(root: &'a Path, dataset: &'a [Value], commit: &'a str) -> DiagnoseInput<'a> {
    fs::create_dir_all(root.join("c1/predictions/run")).unwrap();
    DiagnoseInput {
        entry_id: "absent",
        commit,
        root_dir: root,
        out_dir: root,
        dataset,
        patch_files: &[],
    }
}

#[test]
fn a_swe_diagnosis_names_a_missing_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let err = get_diagnose_prompt_swe(&absent_input(tmp.path(), &[], "c1")).unwrap_err();
    assert!(err.contains("Could not find entry with id absent"), "{err}");
}

#[test]
fn a_polyglot_diagnosis_names_a_missing_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let err = get_diagnose_prompt_polyglot(
        &absent_input(tmp.path(), &[], "c1"),
        &mut rng_drawing(false),
    )
    .unwrap_err();
    assert!(err.contains("Could not find entry with id absent"), "{err}");
}

fn bare_entry() -> [Value; 1] {
    [json!({"instance_id": "absent"})]
}

#[test]
fn a_swe_diagnosis_needs_a_test_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let dataset = bare_entry();
    let err = get_diagnose_prompt_swe(&absent_input(tmp.path(), &dataset, "c1")).unwrap_err();
    assert!(err.contains("no test_patch"), "{err}");
}

#[test]
fn a_polyglot_diagnosis_needs_a_test_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let dataset = bare_entry();
    let err = get_diagnose_prompt_polyglot(
        &absent_input(tmp.path(), &dataset, "c1"),
        &mut rng_drawing(false),
    )
    .unwrap_err();
    assert!(err.contains("no test_patch"), "{err}");
}

#[test]
fn a_swe_diagnosis_of_a_missing_commit_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dataset = bare_entry();
    assert!(get_diagnose_prompt_swe(&absent_input(tmp.path(), &dataset, "nope")).is_err());
}

#[test]
fn a_polyglot_diagnosis_of_a_missing_commit_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dataset = bare_entry();
    assert!(
        get_diagnose_prompt_polyglot(
            &absent_input(tmp.path(), &dataset, "nope"),
            &mut rng_drawing(false)
        )
        .is_err()
    );
}

#[test]
fn value_text_writes_strings_bare() {
    assert_eq!(prompts::value_text(&json!("s")), "s");
}

#[test]
fn value_text_writes_numbers_as_json() {
    assert_eq!(prompts::value_text(&json!(3)), "3");
}

#[test]
fn value_text_writes_null_as_json() {
    assert_eq!(prompts::value_text(&json!(null)), "null");
}

fn odd_statuses() -> String {
    get_eval_log_text(&json!({"i": {"tests_status": {
        "FAIL_TO_PASS": {"success": true, "failure": 2},
        "PASS_TO_PASS": {"success": 1.5, "failure": false}
    }}}))
}

#[test]
fn new_test_statuses_python_cannot_iterate_count_as_empty() {
    let text = odd_statuses();
    assert!(text.contains("Pass All New Tests!"), "{text}");
}

#[test]
fn previous_test_statuses_python_cannot_iterate_count_as_empty() {
    let text = odd_statuses();
    assert!(text.contains("Pass All Previous Tests!"), "{text}");
}

#[test]
fn statuses_python_cannot_iterate_list_no_maintained_tests() {
    let text = odd_statuses();
    assert!(!text.contains("Maintained"), "{text}");
}

#[test]
fn an_eval_log_list_reads_like_an_empty_one() {
    assert_eq!(get_eval_log_text(&json!([])), get_eval_log_text(&json!({})));
}

#[test]
fn relpath_normalizes_dot_and_dotdot() {
    assert_eq!(
        relpath(&PathBuf::from("/x/y/./a/../b.py"), &PathBuf::from("/x/y")),
        "b.py"
    );
}

#[test]
fn relpath_normalizes_the_root_too() {
    assert_eq!(
        relpath(&PathBuf::from("/x/y/a"), &PathBuf::from("/x/y/./c/..")),
        "a"
    );
}

fn unreadable(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, [0xff, 0xfe]).unwrap();
}

fn logs_at(out: &Path, step: usize) -> std::io::Result<EvalLogs> {
    let pred = out.join("c/predictions/run");
    let report = out.join("c/logs/run_evaluation/run/run/i/report.json");
    let steps: [&dyn Fn(); 7] = [
        &|| unreadable(&pred.join("i.md")),
        &|| {
            fs::write(pred.join("i.md"), "ok").unwrap();
            unreadable(&pred.join("i.json"));
        },
        &|| fs::write(pred.join("i.json"), "{").unwrap(),
        &|| {
            fs::write(pred.join("i.json"), "{}").unwrap();
            unreadable(&report);
        },
        &|| fs::write(&report, "[").unwrap(),
        &|| fs::write(&report, "{}").unwrap(),
        &|| unreadable(&pred.join("i_eval.md")),
    ];
    steps.iter().take(step).for_each(|s| s());
    prompts::find_selfimprove_eval_logs("i", out, "c", true)
}

fn logs_err(step: usize) -> bool {
    let tmp = tempfile::tempdir().unwrap();
    logs_at(tmp.path(), step).is_err()
}

#[test]
fn finding_logs_without_predictions_is_an_error() {
    assert!(logs_err(0));
}

#[test]
fn finding_logs_with_an_unreadable_md_log_is_an_error() {
    assert!(logs_err(1));
}

#[test]
fn reading_an_unreadable_md_log_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("i.md");
    unreadable(&path);
    assert!(read_mdlog_file(&path, false).is_err());
}

#[test]
fn finding_logs_with_an_unreadable_prediction_is_an_error() {
    assert!(logs_err(2));
}

#[test]
fn finding_logs_with_a_broken_prediction_is_an_error() {
    assert!(logs_err(3));
}

#[test]
fn finding_logs_with_an_unreadable_report_is_an_error() {
    assert!(logs_err(4));
}

#[test]
fn finding_logs_with_a_broken_report_is_an_error() {
    assert!(logs_err(5));
}

#[test]
fn finding_logs_with_a_report_reads_one_eval_log() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(logs_at(tmp.path(), 6).unwrap().eval_logs.len(), 1);
}

#[test]
fn finding_logs_with_an_unreadable_eval_log_is_an_error() {
    assert!(logs_err(7));
}

#[test]
fn context_length_of_a_missing_commit_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(any_exceeding_context_length(tmp.path(), "missing", &["i"]).is_err());
}

const NONE: [&str; 0] = [];

fn unreadable_code() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    unreadable(&tmp.path().join("pkg/a.rs"));
    tmp
}

#[test]
fn unreadable_code_in_a_folder_is_an_error() {
    let tmp = unreadable_code();
    assert!(prompts::get_current_code(tmp.path(), &["pkg"], &[], &NONE).is_err());
}

#[test]
fn an_unreadable_code_file_is_an_error() {
    let tmp = unreadable_code();
    assert!(prompts::get_current_code(tmp.path(), &["pkg/a.rs"], &[], &NONE).is_err());
}

#[test]
fn a_missing_patch_file_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = [tmp.path().join("missing.diff")];
    assert!(prompts::get_current_code(tmp.path(), &NONE, &missing, &NONE).is_err());
}

#[test]
fn an_excluded_missing_patch_file_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = [tmp.path().join("missing.diff")];
    assert_eq!(
        prompts::get_current_code(tmp.path(), &NONE, &missing, &["missing.diff"]).unwrap(),
        ""
    );
}

fn code_input<'a>(
    root: &'a Path,
    out: &'a Path,
    dataset: &'a [Value],
    missing: &'a [PathBuf],
    entry_id: &'a str,
) -> DiagnoseInput<'a> {
    unreadable(&root.join("pkg/a.rs"));
    fs::create_dir_all(out.join("c/predictions/run")).unwrap();
    DiagnoseInput {
        entry_id,
        commit: "c",
        root_dir: root,
        out_dir: out,
        dataset,
        patch_files: missing,
    }
}

fn without_statement() -> [Value; 1] {
    [json!({"instance_id": "i", "test_patch": "t"})]
}

fn with_statement() -> [Value; 1] {
    [json!({"instance_id": "i", "test_patch": "t", "problem_statement": "p"})]
}

fn diagnose_code(
    dataset: &[Value],
    entry_id: &str,
    polyglot: bool,
) -> Result<(String, String), String> {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let out = root.join("out");
    let missing = [root.join("missing.diff")];
    let input = code_input(root, &out, dataset, &missing, entry_id);
    if polyglot {
        get_diagnose_prompt_polyglot(&input, &mut rng_drawing(false))
    } else {
        get_diagnose_prompt_swe(&input)
    }
}

#[test]
fn a_swe_diagnosis_needs_a_problem_statement() {
    let err = diagnose_code(&without_statement(), "i", false).unwrap_err();
    assert!(err.contains("problem_statement"), "{err}");
}

#[test]
fn a_polyglot_diagnosis_needs_a_problem_statement() {
    let err = diagnose_code(&without_statement(), "i", true).unwrap_err();
    assert!(err.contains("problem_statement"), "{err}");
}

#[test]
fn a_swe_diagnosis_with_a_missing_patch_is_an_error() {
    assert!(diagnose_code(&with_statement(), "i", false).is_err());
}

#[test]
fn a_polyglot_diagnosis_with_a_missing_patch_is_an_error() {
    assert!(diagnose_code(&with_statement(), "i", true).is_err());
}

#[test]
fn an_empty_patch_diagnosis_with_unreadable_code_is_an_error() {
    assert!(diagnose_code(&with_statement(), "solve_empty_patches", false).is_err());
}

#[test]
fn a_diagnosis_without_an_implementation_suggestion_is_an_error() {
    let err =
        get_problem_description_prompt(&json!({"problem_description": "p"}), false).unwrap_err();
    assert!(err.starts_with("missing "), "{err}");
}

#[test]
fn a_diagnosis_without_a_problem_description_is_an_error() {
    let err = get_problem_description_prompt(&json!({"implementation_suggestion": "s"}), false)
        .unwrap_err();
    assert!(err.starts_with("missing "), "{err}");
}

#[test]
fn relpath_of_an_empty_path_is_taken_as_is() {
    assert_eq!(relpath(Path::new(""), Path::new("")), ".");
}
