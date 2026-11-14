use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

use super::{binary, flags, key_file};

const BENCH: &str = "polyglot/polyglot-benchmark";
const CLONE: &str = "polyglot-benchmark";
const COMMITS: &str = "polyglot_benchmark_commits.json";
const DATASET: &str = "polyglot/polyglot_benchmark_metadata.json";
const MARKER: &str = "get_filename_component(exercise ${CMAKE_CURRENT_SOURCE_DIR} NAME)";

fn native(path: &str) -> String {
    path.replace('/', std::path::MAIN_SEPARATOR_STR)
}

fn under(rel: &str) -> String {
    native(&format!("{CLONE}/{rel}"))
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn exercise(bench: &Path, task: &str, config: &Value, files: &[(&str, &str)]) {
    let dir = bench.join(task.replacen('/', "/exercises/practice/", 1));
    write(&dir.join(".meta/config.json"), &config.to_string());
    for (name, text) in files {
        write(&dir.join(name), text);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    if !out.status.success() {
        panic!("{args:?}: {out:?}");
    }
    String::from_utf8(out.stdout).unwrap()
}

fn prepare(root: &Path, repo: &str) -> Output {
    prepare_with(root, Some(repo), None)
}

fn prepare_with(root: &Path, repo: Option<&str>, path: Option<&Path>) -> Output {
    fs::create_dir_all(root).unwrap();
    key_file(root);
    let mut cmd = Command::new(binary("rustyhgm"));
    cmd.args(flags(&[
        ("--polyglot", ""),
        ("--continue_from", "output_rustyhgm/none"),
        ("--polyglot_repo", repo.unwrap_or("no-such-repo")),
    ]))
    .current_dir(root);
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    for key in ["AUTHOR", "COMMITTER"] {
        cmd.env(format!("GIT_{key}_NAME"), "rustyhgm")
            .env(format!("GIT_{key}_EMAIL"), "rustyhgm@example.com");
    }
    cmd.output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stopped_at_the_missing_run(out: &Output) -> Vec<String> {
    let mut wrong = Vec::new();
    if out.status.code() != Some(1) {
        wrong.push(format!("status {:?}", out.status.code()));
    }
    if !stderr(out).contains("rustyhgm_metadata.jsonl") {
        wrong.push(stderr(out));
    }
    wrong
}

fn solution(files: &[&str]) -> Value {
    json!({"files": {"solution": files}})
}

fn hello(bench: &Path) {
    exercise(
        bench,
        "python/hello",
        &json!({
            "authors": ["ann"],
            "blurb": "Say h\u{e9}llo",
            "files": {
                "solution": ["hello.py"],
                "test": ["hello_test.py", "absent_test.py"],
                "example": [".meta/example.py"],
            },
        }),
        &[
            ("hello.py", "def hello(): pass\n"),
            ("hello_test.py", "test\n"),
            (".meta/example.py", "def hello(): return 'hi'\n"),
            (".docs/introduction.md", "Intro"),
            (".docs/instructions.md", "Do it"),
            (".docs/notes.txt", "ignored"),
        ],
    );
}

fn languages(bench: &Path) {
    let cmake = format!("project(x)\n    {MARKER}\n{MARKER}\n");
    exercise(
        bench,
        "cpp/two-fer",
        &json!({"files": {
            "solution": ["two_fer.h", "two_fer.cpp"],
            "test": ["two_fer_test.cpp"],
            "example": [".meta/example.cpp", ".meta/example.h", ".meta/absent.h"],
        }}),
        &[
            ("two_fer.cpp", "a"),
            ("two_fer.h", "b"),
            ("two_fer_test.cpp", "c"),
            (
                ".meta/example.cpp",
                "std::string two_fer::two_fer() { return \"x\"; }\n",
            ),
            (
                ".meta/example.h",
                "#pragma once\nnamespace two_fer {\nstd::string two_fer();\n}\n",
            ),
            ("test/catch.hpp", "d"),
            ("CMakeLists.txt", &cmake),
            (".docs/instructions.md", "Two"),
        ],
    );
    exercise(
        bench,
        "java/bob",
        &solution(&["src/main/java/Bob.java"]),
        &[
            ("src/main/java/Bob.java", "a"),
            ("src/test/java/BobTest.java", "b"),
            ("gradle/wrapper/gradle-wrapper.properties", "c"),
            ("build.gradle", "d"),
            ("gradlew", "e"),
            (".docs/instructions.md", "Bob"),
        ],
    );
    exercise(
        bench,
        "javascript/leap",
        &solution(&["leap.js"]),
        &[
            ("leap.js", "a"),
            ("leap.spec.js", "b"),
            (".eslintrc", "c"),
            ("package.json", "d"),
            (".docs/instructions.md", "Leap"),
        ],
    );
}

fn broken(bench: &Path) {
    exercise(bench, "go/nodocs", &solution(&["a.go"]), &[("a.go", "a")]);
    exercise(
        bench,
        "go/emptydocs",
        &solution(&["a.go"]),
        &[("a.go", "a"), (".docs/notes.txt", "n")],
    );
    exercise(
        bench,
        "go/missing",
        &solution(&["gone.go"]),
        &[(".docs/instructions.md", "Gone")],
    );
    exercise(
        bench,
        "rust/badtest",
        &json!({"files": {"solution": ["a.rs"], "test": ["tests"]}}),
        &[
            ("a.rs", "a"),
            ("tests/t.rs", "t"),
            (".docs/instructions.md", "T"),
        ],
    );
    exercise(
        bench,
        "rust/baddoc",
        &solution(&["a.rs"]),
        &[("a.rs", "a"), (".docs/sub.md/x", "x")],
    );
    exercise(
        bench,
        "rust/badexample",
        &json!({"files": {"solution": ["a.rs"], "example": [".meta/ex"]}}),
        &[
            ("a.rs", "a"),
            (".meta/ex/x", "x"),
            (".docs/instructions.md", "E"),
        ],
    );
    write(&bench.join(".github/exercises/practice/x/a"), "a");
    write(&bench.join("README.md"), "readme");
}

fn tree(dir: &Path, commit: &str) -> Vec<String> {
    git(dir, &["ls-tree", "-r", "--name-only", commit])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn practice(bench: &Path, task: &str) -> std::path::PathBuf {
    bench.join(task.replacen('/', "/exercises/practice/", 1))
}

struct Stage {
    tmp: tempfile::TempDir,
    out: Output,
}

impl Stage {
    fn root(&self) -> &Path {
        self.tmp.path()
    }

    fn bench(&self) -> std::path::PathBuf {
        self.root().join(BENCH)
    }

    fn log(&self) -> String {
        stdout(&self.out)
    }

    fn commits_text(&self) -> String {
        fs::read_to_string(self.bench().join(COMMITS)).unwrap()
    }

    fn commits(&self) -> Value {
        serde_json::from_str(&self.commits_text()).unwrap()
    }

    fn dataset_text(&self) -> String {
        fs::read_to_string(self.root().join(DATASET)).unwrap()
    }

    fn entries(&self) -> Value {
        serde_json::from_str(&self.dataset_text()).unwrap()
    }

    fn base_tree(&self, task: &str) -> Vec<String> {
        let id = task.replacen('/', "__", 1);
        let base = self.commits()[&id][0].as_str().unwrap().to_owned();
        tree(&practice(&self.bench(), task), &base)
    }

    fn then(self, change: impl FnOnce(&Path), repo: Option<&str>, path: Option<&str>) -> Stage {
        change(self.root());
        let path = path.map(|name| {
            let dir = self.root().join(name);
            fs::create_dir_all(&dir).unwrap();
            dir
        });
        let out = prepare_with(self.root(), repo, path.as_deref());
        Stage { out, ..self }
    }
}

fn first() -> Stage {
    let tmp = tempfile::tempdir().unwrap();
    let bench = tmp.path().join(BENCH);
    hello(&bench);
    languages(&bench);
    broken(&bench);
    let out = prepare(tmp.path(), "unused");
    Stage { tmp, out }
}

#[test]
fn a_first_polyglot_preparation_stops_at_the_missing_run() {
    assert_eq!(
        stopped_at_the_missing_run(&first().out),
        Vec::<String>::new()
    );
}

#[test]
fn a_polyglot_run_registers_each_exercise_and_writes_the_dataset() {
    let log = first().log();
    let expected = [
        "Found polyglot-benchmark at ".to_owned(),
        format!("{}{}\n", std::path::MAIN_SEPARATOR, native(BENCH)),
        "Initializing git repositories for 10 practice directories...\n".to_owned(),
        format!(
            "Error initializing git in {}: git add gone.go failed: ",
            under("go/exercises/practice/missing")
        ),
        "Saved commit hashes to ".to_owned(),
        format!("{}\n", under(COMMITS)),
        format!(
            "Error processing {}: Problem statement cannot be empty\n",
            under("go/exercises/practice/emptydocs")
        ),
        format!(
            "Error processing {}: 'go__missing'\n",
            under("go/exercises/practice/missing")
        ),
        format!(
            "Error processing {}: ",
            under("rust/exercises/practice/badtest")
        ),
        format!(
            "Error processing {}: ",
            under("rust/exercises/practice/baddoc")
        ),
        format!(
            "Error processing {}: ",
            under("rust/exercises/practice/badexample")
        ),
        "Generated metadata for 4 tasks\nMetadata written to ".to_owned(),
        format!("{}{}\n", std::path::MAIN_SEPARATOR, native(DATASET)),
    ];
    let missing: Vec<String> = expected
        .into_iter()
        .filter(|line| !log.contains(line.as_str()))
        .collect();
    assert_eq!(missing, Vec::<String>::new(), "{log}");
}

#[test]
fn the_commits_file_is_pretty_printed_in_key_order() {
    let text = first().commits_text();
    assert!(
        text.starts_with("{\n  \"cpp__two-fer\": [\n    \""),
        "{text}"
    );
}

#[test]
fn every_initialized_exercise_has_its_commits() {
    assert_eq!(first().commits().as_object().unwrap().len(), 9);
}

#[test]
fn the_python_base_commit_holds_only_the_starting_files() {
    assert_eq!(
        first().base_tree("python/hello"),
        [
            ".docs/instructions.md",
            ".docs/introduction.md",
            ".docs/notes.txt",
            "hello.py"
        ]
    );
}

#[test]
fn the_cpp_base_commit_holds_only_the_starting_files() {
    assert_eq!(
        first().base_tree("cpp/two-fer"),
        [
            ".docs/instructions.md",
            "CMakeLists.txt",
            "test/catch.hpp",
            "two_fer.cpp",
            "two_fer.h"
        ]
    );
}

#[test]
fn the_cpp_base_commit_names_the_exercise_in_cmake() {
    let stage = first();
    let base = stage.commits()["cpp__two-fer"][0]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        git(
            &practice(&stage.bench(), "cpp/two-fer"),
            &["show", &format!("{base}:CMakeLists.txt")]
        ),
        format!("project(x)\n    set(exercise \"two-fer\")\n{MARKER}\n")
    );
}

#[test]
fn the_java_base_commit_holds_only_the_starting_files() {
    assert_eq!(
        first().base_tree("java/bob"),
        [
            ".docs/instructions.md",
            "build.gradle",
            "gradle/wrapper/gradle-wrapper.properties",
            "gradlew",
            "src/main/java/Bob.java"
        ]
    );
}

#[test]
fn the_javascript_base_commit_holds_only_the_starting_files() {
    assert_eq!(
        first().base_tree("javascript/leap"),
        [
            ".docs/instructions.md",
            ".eslintrc",
            "leap.js",
            "package.json"
        ]
    );
}

#[test]
fn the_test_commit_adds_the_rest_of_the_exercise() {
    let stage = first();
    let test = stage.commits()["python__hello"][1]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        tree(&practice(&stage.bench(), "python/hello"), &test).len(),
        7
    );
}

#[test]
fn the_dataset_escapes_non_ascii_text() {
    assert!(
        first()
            .dataset_text()
            .contains("\"blurb\": \"Say h\\u00e9llo\"")
    );
}

#[test]
fn the_dataset_lists_the_good_exercises_in_order() {
    let entries = first().entries();
    let ids: Vec<&str> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["instance_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "cpp__two-fer",
            "java__bob",
            "javascript__leap",
            "python__hello"
        ]
    );
}

#[test]
fn a_dataset_entry_keeps_the_python_key_order() {
    let entries = first().entries();
    let keys: Vec<&String> = entries[3].as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        [
            "instance_id",
            "language",
            "task_name",
            "files",
            "authors",
            "contributors",
            "blurb",
            "source",
            "source_url",
            "problem_statement",
            "base_commit",
            "test_commit",
            "reference_tests",
            "test_patch",
            "reference_answers",
            "repo"
        ]
    );
}

#[test]
fn a_dataset_entry_describes_its_exercise() {
    let stage = first();
    let commits = stage.commits();
    let entry = &stage.entries()[3];
    let expected = [
        ("instance_id", json!("python__hello")),
        ("language", json!("python")),
        ("task_name", json!("hello")),
        ("authors", json!(["ann"])),
        ("contributors", json!([])),
        ("blurb", json!("Say h\u{e9}llo")),
        ("source", json!("")),
        ("source_url", json!("")),
        ("problem_statement", json!("Do it\n\nIntro\n\n")),
        ("base_commit", commits["python__hello"][0].clone()),
        ("test_commit", commits["python__hello"][1].clone()),
        ("reference_tests", json!("### hello_test.py\ntest\n\n\n")),
        (
            "reference_answers",
            json!("### .meta/example.py\ndef hello(): return 'hi'\n\n\n"),
        ),
        ("repo", json!(under("python/exercises/practice/hello"))),
    ];
    let wrong: Vec<String> = expected
        .into_iter()
        .filter(|(key, value)| entry[*key] != *value)
        .map(|(key, value)| format!("{key}: {} != {value}", entry[key]))
        .collect();
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn a_cpp_problem_statement_ends_with_the_declarations_the_tests_call() {
    assert_eq!(
        first().entries()[0]["problem_statement"],
        "Two\n\n# Interface\n\nThe tests call the solution through the declarations below. Declare each one with exactly this name, these parameters and this return type. Everything else is yours to design.\n\n```cpp\nnamespace two_fer {\n    std::string two_fer();\n}\n```\n\n"
    );
}

#[test]
fn a_problem_statement_in_another_language_gains_nothing() {
    assert_eq!(first().entries()[1]["problem_statement"], "Bob\n\n");
}

#[test]
fn a_dataset_entry_keeps_the_configured_test_files() {
    assert_eq!(
        first().entries()[3]["files"]["test"],
        json!(["hello_test.py", "absent_test.py"])
    );
}

fn hello_patch() -> String {
    first().entries()[3]["test_patch"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn the_test_patch_adds_the_tests() {
    assert!(hello_patch().contains("+++ b/hello_test.py"));
}

#[test]
fn the_test_patch_adds_the_example_answer() {
    assert!(hello_patch().contains("+++ b/.meta/example.py"));
}

#[test]
fn reference_tests_are_read_from_the_test_files() {
    assert_eq!(
        first().entries()[0]["reference_tests"],
        "### two_fer_test.cpp\nc\n\n"
    );
}

#[test]
fn an_exercise_without_test_files_has_no_reference_tests() {
    assert_eq!(first().entries()[1]["reference_tests"], "");
}

fn again() -> Stage {
    first().then(|_| {}, None, None)
}

#[test]
fn a_second_preparation_stops_at_the_missing_run() {
    assert_eq!(
        stopped_at_the_missing_run(&again().out),
        Vec::<String>::new()
    );
}

#[test]
fn a_second_preparation_finds_the_repositories_ready() {
    assert!(
        again()
            .log()
            .contains("All practice directories already initialized with git\n")
    );
}

#[test]
fn a_second_preparation_writes_the_same_dataset() {
    let stage = first();
    let text = stage.dataset_text();
    assert_eq!(stage.then(|_| {}, None, None).dataset_text(), text);
}

fn reconfigure(root: &Path) {
    let bench = root.join(BENCH);
    fs::write(practice(&bench, "java/bob").join(".meta/config.json"), "{}").unwrap();
    fs::write(
        practice(&bench, "javascript/leap").join(".meta/config.json"),
        "{",
    )
    .unwrap();
}

fn later() -> Stage {
    again().then(reconfigure, Some("unused"), None)
}

#[test]
fn a_later_preparation_stops_at_the_missing_run() {
    assert_eq!(
        stopped_at_the_missing_run(&later().out),
        Vec::<String>::new()
    );
}

#[test]
fn a_later_preparation_reports_a_broken_config() {
    assert!(later().log().contains(&format!(
        "Error processing {}: ",
        under("javascript/exercises/practice/leap")
    )));
}

#[test]
fn a_later_preparation_drops_the_broken_exercise() {
    assert!(later().log().contains("Generated metadata for 3 tasks\n"));
}

#[test]
fn a_later_preparation_keeps_an_exercise_with_an_empty_config() {
    assert_eq!(later().entries()[1]["instance_id"], "java__bob");
}

#[test]
fn an_empty_config_lists_no_files() {
    assert_eq!(later().entries()[1]["files"], json!({}));
}

#[test]
fn an_empty_config_has_no_reference_tests() {
    assert_eq!(later().entries()[1]["reference_tests"], "");
}

fn gitless() -> Stage {
    later().then(|_| {}, Some("unused"), Some("no-git"))
}

#[test]
fn a_preparation_without_git_stops_at_the_missing_run() {
    assert_eq!(
        stopped_at_the_missing_run(&gitless().out),
        Vec::<String>::new()
    );
}

#[test]
fn a_preparation_without_git_reports_each_exercise() {
    assert!(gitless().log().contains(&format!(
        "Error processing {}: ",
        under("python/exercises/practice/hello")
    )));
}

#[test]
fn a_preparation_without_git_writes_no_entries() {
    assert!(gitless().log().contains("Generated metadata for 0 tasks\n"));
}

fn corrupt_commits() -> Stage {
    later().then(
        |root| fs::write(root.join(BENCH).join(COMMITS), "{").unwrap(),
        Some("unused"),
        None,
    )
}

#[test]
fn a_corrupt_commits_file_fails_the_run() {
    assert_eq!(corrupt_commits().out.status.code(), Some(1));
}

#[test]
fn a_corrupt_commits_file_is_named() {
    assert!(stderr(&corrupt_commits().out).contains("polyglot_benchmark_commits.json: "));
}

fn lost_commits() -> Stage {
    later().then(
        |root| fs::remove_file(root.join(BENCH).join(COMMITS)).unwrap(),
        Some("unused"),
        None,
    )
}

#[test]
fn a_lost_commits_file_fails_the_run() {
    assert_eq!(lost_commits().out.status.code(), Some(1));
}

#[test]
fn a_lost_commits_file_is_reported() {
    let e = stderr(&lost_commits().out);
    assert!(e.starts_with("rustyhgm: Commits file not found: "), "{e}");
}

fn source_repo(source: &Path) {
    hello(source);
    exercise(
        source,
        "cpp/nocmake",
        &solution(&["a.cpp"]),
        &[("a.cpp", "a"), (".docs/instructions.md", "N")],
    );
    git(source, &["init", "-q"]);
    git(source, &["add", "."]);
    git(
        source,
        &[
            "-c",
            "user.name=rustyhgm",
            "-c",
            "user.email=rustyhgm@example.com",
            "commit",
            "-qm",
            "benchmark",
        ],
    );
}

fn cloned() -> Output {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    source_repo(&source);
    let root = tmp.path().join("root");
    fs::create_dir(&root).unwrap();
    prepare(&root, &source.to_string_lossy())
}

#[test]
fn a_cloning_preparation_stops_at_the_missing_run() {
    assert_eq!(stopped_at_the_missing_run(&cloned()), Vec::<String>::new());
}

#[test]
fn a_polyglot_run_clones_the_benchmark_when_it_is_missing() {
    assert!(
        stdout(&cloned())
            .strip_prefix("Cloning polyglot-benchmark repository to ")
            .is_some_and(|rest| rest.contains(&format!("{} ...\n", native(BENCH))))
    );
}

#[test]
fn a_cloned_benchmark_is_prepared() {
    assert!(stdout(&cloned()).contains("Generated metadata for 1 tasks\n"));
}

#[test]
fn a_cpp_exercise_without_cmake_cannot_be_initialized() {
    assert!(stdout(&cloned()).contains(&format!(
        "Error initializing git in {}: ",
        under("cpp/exercises/practice/nocmake")
    )));
}

fn clone_failure(path: Option<&str>) -> Output {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let path = path.map(|name| {
        let dir = tmp.path().join(name);
        fs::create_dir(&dir).unwrap();
        dir
    });
    let repo = tmp.path().join("nowhere");
    prepare_with(&empty, Some(&repo.to_string_lossy()), path.as_deref())
}

#[test]
fn a_failing_clone_fails_the_run() {
    assert_eq!(clone_failure(None).status.code(), Some(1));
}

#[test]
fn a_failing_clone_is_reported() {
    let e = stderr(&clone_failure(None));
    assert!(e.starts_with("rustyhgm: git clone "), "{e}");
}

#[test]
fn a_clone_without_git_fails_the_run() {
    assert_eq!(clone_failure(Some("no-git")).status.code(), Some(1));
}

#[test]
fn a_clone_without_git_is_reported() {
    let e = stderr(&clone_failure(Some("no-git")));
    assert!(e.starts_with("rustyhgm: "), "{e}");
}

fn stop(name: &str, prepare_root: impl FnOnce(&Path), expected: &str) -> Option<String> {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join(name);
    prepare_root(&root);
    let out = prepare(&root, "unused");
    let e = stderr(&out);
    if out.status.code() == Some(1) && e.contains(expected) {
        None
    } else {
        Some(format!("{name}: {:?} {e}", out.status.code()))
    }
}

fn config(root: &Path, config: Option<&str>) {
    let dir = practice(&root.join(BENCH), "go/x");
    fs::create_dir_all(dir.join(".meta")).unwrap();
    if let Some(config) = config {
        fs::write(dir.join(".meta/config.json"), config).unwrap();
    }
}

#[test]
fn a_polyglot_run_stops_on_a_bad_exercise_config() {
    let wrong: Vec<String> = [
        ("invalid", Some("{"), "config.json: "),
        ("absent", None, "Config file not found: "),
        (
            "unsolved",
            Some("{\"files\": {}}"),
            "No solution files found in config for ",
        ),
    ]
    .into_iter()
    .filter_map(|(name, text, expected)| stop(name, |root| config(root, text), expected))
    .collect();
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn a_polyglot_run_stops_on_an_unwritable_output() {
    let wrong: Vec<String> = [
        (DATASET.to_owned(), native(DATASET)),
        (format!("{BENCH}/{COMMITS}"), under(COMMITS)),
    ]
    .into_iter()
    .filter_map(|(blocked, shown)| {
        stop(
            &blocked.replace('/', "_"),
            |root| {
                hello(&root.join(BENCH));
                fs::create_dir_all(root.join(&blocked)).unwrap();
            },
            &format!("{shown}: "),
        )
    })
    .collect();
    assert_eq!(wrong, Vec::<String>::new());
}
