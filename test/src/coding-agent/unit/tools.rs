use std::fs;
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use rustyhgm_coding_agent::tools::{
    Tools, bash_info, editor, editor_info, expand_tabs, filter_error, format_output,
    maybe_truncate, tool_infos,
};
use serde_json::{Value, json};

use crate::common::cases;

fn case(name: &str) -> Value {
    cases("tools")
        .into_iter()
        .find(|c| c["name"] == name)
        .unwrap()
}

#[test]
fn the_bash_tool_info_matches_the_python() {
    assert_eq!(bash_info(), case("bash_tool_info")["output"]);
}

#[test]
fn the_editor_tool_info_matches_the_python() {
    assert_eq!(editor_info(), case("editor_tool_info")["output"]);
}

#[test]
fn tool_infos_list_bash_then_the_editor() {
    assert_eq!(tool_infos(), [bash_info(), editor_info()]);
}

fn long_view() -> String {
    format_output(&"y".repeat(10010), "/repo/f.py")
}

#[test]
fn a_long_file_view_is_clipped_to_the_python_length() {
    let c = case("format_output_long_truncated");
    assert_eq!(
        long_view().len() as u64,
        c["output"]["len"].as_u64().unwrap()
    );
}

#[test]
fn a_long_file_view_ends_like_the_python() {
    let c = case("format_output_long_truncated");
    assert!(long_view().ends_with(c["output"]["tail"].as_str().unwrap()));
}

#[test]
fn file_views_match_the_python() {
    let mismatches: Vec<String> = cases("tools")
        .iter()
        .filter(|c| {
            let name = c["name"].as_str().unwrap();
            name.starts_with("format_output_") && name != "format_output_long_truncated"
        })
        .filter_map(|c| {
            let out = format_output(c["input"].as_str().unwrap(), "/repo/f.py");
            (out != c["output"].as_str().unwrap()).then(|| format!("{}: {out:?}", c["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn truncation_keeps_ten_thousand_characters() {
    assert_eq!(maybe_truncate(&"é".repeat(10000)), "é".repeat(10000));
}

#[test]
fn truncation_counts_characters() {
    assert_eq!(
        maybe_truncate(&"é".repeat(10001)),
        format!("{}\n<response clipped>", "é".repeat(10000))
    );
}

#[test]
fn tabs_reset_on_new_lines() {
    assert_eq!(
        expand_tabs("ab\tc\r\td\n\te"),
        "ab      c\r        d\n        e"
    );
}

fn editor_calls(tmp: &Path) -> Vec<String> {
    let root = tmp.to_str().unwrap();
    fs::write(tmp.join("f.txt"), "x\ny\n").unwrap();
    let tools = Tools::default();
    let native = |text: &str| {
        text.replace("<TMP>/", &format!("{root}{MAIN_SEPARATOR}"))
            .replace("<TMP>", root)
    };
    cases("tools")
        .iter()
        .filter(|c| {
            let name = c["name"].as_str().unwrap();
            name.starts_with("edit_tool_function_") && !name.ends_with("_files_after")
        })
        .filter_map(|c| {
            let mut input = c["input"].clone();
            for value in input.as_object_mut().unwrap().values_mut() {
                if let Some(text) = value.as_str() {
                    *value = json!(native(text));
                }
            }
            let got = tools.call("editor", &input);
            (got != native(c["output"].as_str().unwrap()))
                .then(|| format!("{}: {got:?}", c["name"]))
        })
        .collect()
}

#[test]
fn editor_calls_match_the_python() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(editor_calls(tmp.path()), Vec::<String>::new());
}

#[test]
fn editor_calls_leave_the_files_the_python_leaves() {
    let tmp = tempfile::tempdir().unwrap();
    editor_calls(tmp.path());
    let mut after = serde_json::Map::new();
    for entry in fs::read_dir(tmp.path()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        after.insert(name, json!(fs::read_to_string(&path).unwrap()));
    }
    let mut want = case("edit_tool_function_files_after")["output"].clone();
    want.as_object_mut().unwrap().sort_keys();
    after.sort_keys();
    assert_eq!(Value::Object(after), want);
}

fn view_tree() -> (tempfile::TempDir, String, String) {
    let tmp = tempfile::Builder::new().prefix("view").tempdir().unwrap();
    let root = tmp.path();
    for dir in ["b/c/d", ".hidden", "a"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "z.txt",
        "b/c/d/deep.txt",
        "b/one.txt",
        ".hidden/x",
        "b/.dot",
    ] {
        fs::write(root.join(file), "").unwrap();
    }
    let shown = root.display().to_string();
    let listing = ["", "a", "b", "b/c", "b/one.txt", "z.txt"]
        .map(|p| {
            if p.is_empty() {
                shown.clone()
            } else {
                root.join(p)
                    .components()
                    .collect::<PathBuf>()
                    .display()
                    .to_string()
            }
        })
        .join("\n");
    let want = format!(
        "Here's the files and directories up to 2 levels deep in {shown}, excluding hidden items:\n{listing}\n"
    );
    (tmp, shown, want)
}

#[test]
fn directories_list_two_levels_without_hidden_entries() {
    let (_tmp, shown, want) = view_tree();
    assert_eq!(editor(Some("view"), &shown, None), want);
}

#[test]
fn a_trailing_slash_lists_the_same_directory() {
    let (_tmp, shown, want) = view_tree();
    assert_eq!(editor(Some("view"), &format!("{shown}/"), None), want);
}

#[test]
fn a_hidden_directory_lists_nothing_inside() {
    let (tmp, _, _) = view_tree();
    let hidden = tmp.path().join(".hidden").display().to_string();
    assert_eq!(
        editor(Some("view"), &hidden, None),
        format!(
            "Here's the files and directories up to 2 levels deep in {hidden}, excluding hidden items:\n\n"
        )
    );
}

#[cfg(unix)]
#[test]
fn an_unreadable_directory_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok();
    let out = editor(Some("view"), tmp.path().to_str().unwrap(), None);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        readable || out.starts_with("Error listing directory: "),
        "{out}"
    );
}

#[test]
fn an_unreadable_file_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.bin");
    fs::write(&bad, [0xff, 0xfe]).unwrap();
    let out = editor(Some("view"), bad.to_str().unwrap(), None);
    assert!(out.starts_with("Error: Failed to read file: "), "{out}");
}

#[test]
fn writes_into_missing_directories_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("missing/new.txt");
    let out = editor(Some("create"), path.to_str().unwrap(), Some("x"));
    assert!(out.starts_with("Error: Failed to write file: "), "{out}");
}

fn call(name: &str, args: &Value) -> String {
    Tools::default().call(name, args)
}

#[test]
fn an_unknown_tool_is_reported() {
    assert_eq!(call("rm", &json!({})), "Error: Tool 'rm' not found");
}

#[test]
fn a_null_editor_command_is_unsupported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    assert_eq!(
        call("editor", &json!({"path": path, "command": null})),
        "Error: Unknown or unsupported command: None"
    );
}

#[test]
fn an_editor_call_without_a_path_is_reported() {
    assert_eq!(
        call("editor", &json!({"command": "view"})),
        "Error executing tool 'editor': missing required argument 'path'"
    );
}

#[test]
fn a_null_editor_path_is_reported() {
    assert_eq!(
        call("editor", &json!({"command": "view", "path": null})),
        "Error executing tool 'editor': argument 'path' must be a string, not null"
    );
}

#[test]
fn a_numeric_editor_path_is_reported() {
    assert_eq!(
        call("editor", &json!({"command": "view", "path": 3})),
        "Error executing tool 'editor': argument 'path' must be a string, not 3"
    );
}

#[test]
fn an_unexpected_editor_argument_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    assert_eq!(
        call("editor", &json!({"command": "view", "path": path, "x": 1})),
        "Error executing tool 'editor': unexpected argument 'x'"
    );
}

#[test]
fn bash_arguments_that_are_not_an_object_are_reported() {
    assert_eq!(
        call("bash", &json!([1])),
        "Error executing tool 'bash': arguments must be an object, not [1]"
    );
}

#[test]
fn a_bash_call_without_a_command_is_reported() {
    assert_eq!(
        call("bash", &Value::Null),
        "Error executing tool 'bash': missing required argument 'command'"
    );
}

#[test]
fn a_null_bash_command_is_reported() {
    assert_eq!(
        call("bash", &json!({"command": null})),
        "Error executing tool 'bash': argument 'command' must be a string, not null"
    );
}

#[test]
fn bash_errors_are_filtered_like_the_python() {
    let mismatches: Vec<String> = cases("tools")
        .iter()
        .filter(|c| {
            c["name"]
                .as_str()
                .unwrap()
                .starts_with("bash_filter_error_")
        })
        .filter_map(|c| {
            let got = filter_error(c["input"].as_str().unwrap());
            (got != c["output"].as_str().unwrap()).then(|| format!("{}: {got:?}", c["name"]))
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn ioctl_noise_is_filtered_from_bash_errors() {
    assert_eq!(filter_error("a\nInappropriate ioctl for device"), "a");
}
