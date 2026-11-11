use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

use super::binary;

const BENCH: &str = "polyglot/polyglot-benchmark";
const MARKER: &str = "get_filename_component(exercise ${CMAKE_CURRENT_SOURCE_DIR} NAME)";

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
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn prepare(root: &Path, repo: &str) -> Output {
    let mut cmd = Command::new(binary("hgm"));
    cmd.arg("prepare-polyglot")
        .current_dir(root)
        .env("HGM_POLYGLOT_REPO", repo);
    for key in ["AUTHOR", "COMMITTER"] {
        cmd.env(format!("GIT_{key}_NAME"), "hgm")
            .env(format!("GIT_{key}_EMAIL"), "hgm@example.com");
    }
    cmd.output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn failure(out: &Output) -> String {
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
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
        &json!({"files": {"solution": ["two_fer.h", "two_fer.cpp"], "test": ["two_fer_test.cpp"]}}),
        &[
            ("two_fer.cpp", "a"),
            ("two_fer.h", "b"),
            ("two_fer_test.cpp", "c"),
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

fn check_first_commits(bench: &Path, commits: &Value) {
    let base = |id: &str| commits[id][0].as_str().unwrap().to_owned();
    let hello = practice(bench, "python/hello");
    assert_eq!(
        tree(&hello, &base("python__hello")),
        [
            ".docs/instructions.md",
            ".docs/introduction.md",
            ".docs/notes.txt",
            "hello.py"
        ]
    );
    let fer = practice(bench, "cpp/two-fer");
    assert_eq!(
        tree(&fer, &base("cpp__two-fer")),
        [
            ".docs/instructions.md",
            "CMakeLists.txt",
            "test/catch.hpp",
            "two_fer.cpp",
            "two_fer.h"
        ]
    );
    assert_eq!(
        git(
            &fer,
            &["show", &format!("{}:CMakeLists.txt", base("cpp__two-fer"))]
        ),
        format!("project(x)\n    set(exercise \"two-fer\")\n{MARKER}\n")
    );
    assert_eq!(
        tree(&practice(bench, "java/bob"), &base("java__bob")),
        [
            ".docs/instructions.md",
            "build.gradle",
            "gradle/wrapper/gradle-wrapper.properties",
            "gradlew",
            "src/main/java/Bob.java"
        ]
    );
    assert_eq!(
        tree(
            &practice(bench, "javascript/leap"),
            &base("javascript__leap")
        ),
        [
            ".docs/instructions.md",
            ".eslintrc",
            "leap.js",
            "package.json"
        ]
    );
    assert_eq!(
        tree(&hello, commits["python__hello"][1].as_str().unwrap()).len(),
        7
    );
}

fn check_hello(entry: &Value, commits: &Value) {
    let keys: Vec<&String> = entry.as_object().unwrap().keys().collect();
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
    check_hello_fields(entry, commits);
}

fn check_hello_fields(entry: &Value, commits: &Value) {
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
        (
            "repo",
            json!(format!("{BENCH}/python/exercises/practice/hello")),
        ),
    ];
    for (key, value) in expected {
        assert_eq!(entry[key], value, "{key}");
    }
    assert_eq!(
        entry["files"]["test"],
        json!(["hello_test.py", "absent_test.py"])
    );
    let patch = entry["test_patch"].as_str().unwrap();
    assert!(patch.contains("+++ b/hello_test.py"), "{patch}");
    assert!(patch.contains("+++ b/.meta/example.py"), "{patch}");
}

#[test]
fn prepare_polyglot_registers_each_exercise_and_writes_the_dataset() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let bench = root.join(BENCH);
    hello(&bench);
    languages(&bench);
    broken(&bench);
    let out = prepare(root, "unused");
    assert!(out.status.success(), "{out:?}");
    let log = stdout(&out);
    for line in [
        format!("Found polyglot-benchmark at {BENCH}\n"),
        "Initializing git repositories for 9 practice directories...\n".to_owned(),
        format!("Error initializing git in {BENCH}/go/exercises/practice/missing: git add gone.go failed: "),
        "Saved commit hashes to ".to_owned(),
        format!("{BENCH}/polyglot_benchmark_commits.json\n"),
        format!("Error processing {BENCH}/go/exercises/practice/emptydocs: Problem statement cannot be empty\n"),
        format!("Error processing {BENCH}/go/exercises/practice/missing: 'go__missing'\n"),
        format!("Error processing {BENCH}/rust/exercises/practice/badtest: "),
        format!("Error processing {BENCH}/rust/exercises/practice/baddoc: "),
        "Generated metadata for 4 tasks\nMetadata written to polyglot/polyglot_benchmark_metadata.json\n".to_owned(),
    ] {
        assert!(log.contains(&line), "{line}\n---\n{log}");
    }
    let commits_text = fs::read_to_string(bench.join("polyglot_benchmark_commits.json")).unwrap();
    assert!(
        commits_text.starts_with("{\n  \"cpp__two-fer\": [\n    \""),
        "{commits_text}"
    );
    let commits: Value = serde_json::from_str(&commits_text).unwrap();
    assert_eq!(commits.as_object().unwrap().len(), 8);
    check_first_commits(&bench, &commits);
    let dataset = root.join("polyglot/polyglot_benchmark_metadata.json");
    let text = fs::read_to_string(&dataset).unwrap();
    assert!(text.contains("\"blurb\": \"Say h\\u00e9llo\""), "{text}");
    let entries: Value = serde_json::from_str(&text).unwrap();
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
    check_hello(&entries[3], &commits);
    assert_eq!(entries[0]["reference_tests"], "### two_fer_test.cpp\nc\n\n");
    assert_eq!(entries[1]["reference_tests"], "");

    let again = prepare(root, "unused");
    assert!(again.status.success(), "{again:?}");
    assert!(stdout(&again).contains("All practice directories already initialized with git\n"));
    assert_eq!(fs::read_to_string(&dataset).unwrap(), text);

    let commits_path = bench.join("polyglot_benchmark_commits.json");
    fs::write(&commits_path, "{").unwrap();
    assert!(failure(&prepare(root, "unused")).contains("polyglot_benchmark_commits.json: "));
    fs::remove_file(&commits_path).unwrap();
    let e = failure(&prepare(root, "unused"));
    assert!(e.starts_with("hgm: Commits file not found: "), "{e}");
}

#[test]
fn prepare_polyglot_clones_the_benchmark_when_it_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    hello(&source);
    git(&source, &["init", "-q"]);
    git(&source, &["add", "."]);
    git(
        &source,
        &[
            "-c",
            "user.name=hgm",
            "-c",
            "user.email=hgm@example.com",
            "commit",
            "-qm",
            "benchmark",
        ],
    );
    let root = tmp.path().join("root");
    fs::create_dir(&root).unwrap();
    let out = prepare(&root, &source.to_string_lossy());
    assert!(out.status.success(), "{out:?}");
    let log = stdout(&out);
    assert!(
        log.starts_with(&format!(
            "Cloning polyglot-benchmark repository to {BENCH} ...\n"
        )),
        "{log}"
    );
    assert!(log.contains("Generated metadata for 1 tasks\n"), "{log}");

    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let e = failure(&prepare(
        &empty,
        &tmp.path().join("nowhere").to_string_lossy(),
    ));
    assert!(e.starts_with("hgm: git clone "), "{e}");
}

#[test]
fn prepare_polyglot_stops_on_a_bad_config_or_an_unwritable_output() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, config, expected) in [
        ("invalid", Some("{"), "config.json: "),
        ("absent", None, "Config file not found: "),
        (
            "unsolved",
            Some("{\"files\": {}}"),
            "No solution files found in config for ",
        ),
    ] {
        let root = tmp.path().join(name);
        let dir = practice(&root.join(BENCH), "go/x");
        fs::create_dir_all(dir.join(".meta")).unwrap();
        if let Some(config) = config {
            fs::write(dir.join(".meta/config.json"), config).unwrap();
        }
        let e = failure(&prepare(&root, "unused"));
        assert!(e.contains(expected), "{name}: {e}");
    }
    for blocked in [
        "polyglot/polyglot_benchmark_metadata.json",
        "polyglot/polyglot-benchmark/polyglot_benchmark_commits.json",
    ] {
        let root = tmp.path().join(blocked.replace('/', "_"));
        hello(&root.join(BENCH));
        fs::create_dir_all(root.join(blocked)).unwrap();
        let e = failure(&prepare(&root, "unused"));
        assert!(e.contains(&format!("{blocked}: ")), "{blocked}: {e}");
    }
}
