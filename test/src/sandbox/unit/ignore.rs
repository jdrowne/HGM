use std::fs;

use rustyhgm_sandbox::Error;
use rustyhgm_sandbox::ignore::{Spec, parse_dockerignore, read_dockerignore};
use serde_json::Value;

use crate::{case, strings};

type Pairs<T> = (Vec<(String, T)>, Vec<(String, T)>);

#[test]
fn parses_the_repo_dockerignore() {
    let c = case("dockerignore", "read_dockerignore_repo");
    assert_eq!(
        parse_dockerignore(c["input"].as_str().unwrap()),
        strings(&c["output"])
    );
}

#[test]
fn parses_comments_blank_lines_and_padding() {
    let c = case("dockerignore", "read_dockerignore_synthetic");
    assert_eq!(
        parse_dockerignore(c["input"].as_str().unwrap()),
        strings(&c["output"])
    );
}

#[test]
fn missing_dockerignore_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        read_dockerignore(&dir.path().join(".dockerignore"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn match_file_agrees_with_pathspec() {
    let c = case("dockerignore", "gitwildmatch_match_file");
    let spec = Spec::new(&strings(&c["input"]["patterns"])).unwrap();
    let (actual, expected): Pairs<bool> = c["output"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, matched)| {
            (
                (path.clone(), spec.matches(path)),
                (path.clone(), matched.as_bool().unwrap()),
            )
        })
        .unzip();
    assert_eq!(actual, expected);
}

#[test]
fn walk_agrees_with_os_walk() {
    let c = case("dockerignore", "files_respecting_dockerignore");
    let patterns = strings(&c["input"]["patterns"]);
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for f in strings(&c["input"]["files"]) {
        if f == "build" {
            continue;
        }
        let p = root.join(&f);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, &f).unwrap();
    }
    fs::write(root.join(".dockerignore"), patterns.join("\n") + "\n").unwrap();
    let spec = Spec::new(&patterns).unwrap();
    let kept: Vec<String> = spec.walk(root).into_iter().collect();
    assert_eq!(kept, strings(&c["output"]));
}

#[test]
fn ignored_directories_are_not_entered() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("skip/inner")).unwrap();
    fs::write(root.join("skip/inner/keep.txt"), "").unwrap();
    let spec = Spec::new(&["skip/", "!skip/inner/keep.txt"]).unwrap();
    assert!(spec.walk(root).is_empty());
}

fn glob_spec() -> Spec {
    Spec::new(&["a?c", "[!x]y.txt", "lit\\*", "/rooted/**/deep"]).unwrap()
}

#[test]
fn a_question_mark_matches_one_character() {
    assert!(glob_spec().matches("abc"));
}

#[test]
fn a_question_mark_pattern_matches_in_subdirectories() {
    assert!(glob_spec().matches("sub/abc"));
}

#[test]
fn a_question_mark_does_not_match_a_slash() {
    assert!(!glob_spec().matches("a/c"));
}

#[test]
fn a_negated_class_matches_other_characters() {
    assert!(glob_spec().matches("zy.txt"));
}

#[test]
fn a_negated_class_rejects_its_characters() {
    assert!(!glob_spec().matches("xy.txt"));
}

#[test]
fn an_escaped_star_matches_a_literal_star() {
    assert!(glob_spec().matches("lit*"));
}

#[test]
fn an_escaped_star_is_not_a_wildcard() {
    assert!(!glob_spec().matches("litx"));
}

#[test]
fn a_rooted_double_star_matches_no_directories() {
    assert!(glob_spec().matches("rooted/deep"));
}

#[test]
fn a_rooted_double_star_matches_several_directories() {
    assert!(glob_spec().matches("rooted/a/b/deep"));
}

#[test]
fn a_rooted_pattern_does_not_match_below_the_root() {
    assert!(!glob_spec().matches("x/rooted/deep"));
}

#[test]
fn patterns_that_match_nothing_leave_the_spec_empty() {
    assert!(
        Spec::new(&["/", "# c", "", "[unclosed"])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_double_star_matches_everything() {
    assert!(Spec::new(&["**"]).unwrap().matches("any/thing"));
}

#[test]
fn a_double_star_directory_matches_files_inside_directories() {
    assert!(Spec::new(&["**/"]).unwrap().matches("dir/file"));
}

#[test]
fn a_double_star_directory_does_not_match_top_level_files() {
    assert!(!Spec::new(&["**/"]).unwrap().matches("file"));
}

#[test]
fn a_bare_negation_is_a_pattern_error() {
    assert!(matches!(Spec::new(&["!"]), Err(Error::Pattern(_))));
}

#[test]
fn a_trailing_backslash_is_a_pattern_error() {
    assert!(matches!(Spec::new(&["trailing\\"]), Err(Error::Pattern(_))));
}

fn edge_outcome(pattern: &str, expected: &Value) -> Value {
    match (Spec::new(&[pattern]), expected.as_object()) {
        (Ok(spec), Some(paths)) => Value::Object(
            paths
                .keys()
                .map(|path| (path.clone(), Value::Bool(spec.matches(path))))
                .collect(),
        ),
        (Ok(_), None) => Value::String(format!("{pattern:?} was accepted")),
        (Err(e), _) => Value::String(e.to_string()),
    }
}

fn expected_edge_outcome(pattern: &str, expected: &Value) -> Value {
    if expected.is_object() {
        expected.clone()
    } else {
        Value::String(Error::Pattern(pattern.to_owned()).to_string())
    }
}

#[test]
fn edge_patterns_agree_with_pathspec() {
    let doc: Value = serde_json::from_str(
        &fs::read_to_string(crate::fixtures_dir().join("dockerignore.json")).unwrap(),
    )
    .unwrap();
    let (actual, expected): Pairs<Value> = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] == "gitwildmatch_edge")
        .map(|c| {
            let pattern = c["input"].as_str().unwrap();
            (
                (pattern.to_owned(), edge_outcome(pattern, &c["output"])),
                (
                    pattern.to_owned(),
                    expected_edge_outcome(pattern, &c["output"]),
                ),
            )
        })
        .unzip();
    assert_eq!(actual, expected);
}

#[test]
fn a_dockerignore_that_cannot_be_read_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".dockerignore")).unwrap();
    assert!(read_dockerignore(&dir.path().join(".dockerignore")).is_err());
}

#[test]
fn walking_a_missing_root_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Spec::default().walk(&dir.path().join("missing")).is_empty());
}

#[cfg(unix)]
#[test]
fn symlinked_directories_are_listed_but_not_entered() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join("real")).unwrap();
    fs::write(root.join("real/f"), "").unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
    let files: Vec<String> = Spec::default().walk(&root).into_iter().collect();
    assert_eq!(files, ["real/f"]);
}
