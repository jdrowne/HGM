use std::fs;

use hgm_sandbox::Error;
use hgm_sandbox::ignore::{Spec, parse_dockerignore, read_dockerignore};

use crate::{case, strings};

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
    for (path, expected) in c["output"].as_object().unwrap() {
        assert_eq!(spec.matches(path), expected.as_bool().unwrap(), "{path}");
    }
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

#[test]
fn glob_features_translate() {
    let spec = Spec::new(&["a?c", "[!x]y.txt", "lit\\*", "/rooted/**/deep"]).unwrap();
    assert!(spec.matches("abc"));
    assert!(spec.matches("sub/abc"));
    assert!(!spec.matches("a/c"));
    assert!(spec.matches("zy.txt"));
    assert!(!spec.matches("xy.txt"));
    assert!(spec.matches("lit*"));
    assert!(!spec.matches("litx"));
    assert!(spec.matches("rooted/deep"));
    assert!(spec.matches("rooted/a/b/deep"));
    assert!(!spec.matches("x/rooted/deep"));
}

#[test]
fn degenerate_patterns() {
    assert!(
        Spec::new(&["/", "# c", "", "[unclosed"])
            .unwrap()
            .is_empty()
    );
    assert!(Spec::new(&["**"]).unwrap().matches("any/thing"));
    assert!(Spec::new(&["**/"]).unwrap().matches("dir/file"));
    assert!(!Spec::new(&["**/"]).unwrap().matches("file"));
    assert!(matches!(Spec::new(&["!"]), Err(Error::Pattern(_))));
    assert!(matches!(Spec::new(&["trailing\\"]), Err(Error::Pattern(_))));
}

#[test]
fn edge_patterns_agree_with_pathspec() {
    let doc: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(crate::fixtures_dir().join("dockerignore.json")).unwrap(),
    )
    .unwrap();
    for c in doc["cases"].as_array().unwrap() {
        if c["name"] != "gitwildmatch_edge" {
            continue;
        }
        let pattern = c["input"].as_str().unwrap();
        match (Spec::new(&[pattern]), c["output"].as_object()) {
            (Ok(spec), Some(expected)) => {
                for (path, matched) in expected {
                    assert_eq!(
                        spec.matches(path),
                        matched.as_bool().unwrap(),
                        "{pattern:?} on {path:?}"
                    );
                }
            }
            (Err(Error::Pattern(p)), None) => assert_eq!(p, pattern),
            (got, _) => panic!("{pattern:?} gave {got:?}, expected {}", c["output"]),
        }
    }
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
