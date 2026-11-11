use std::fs;
use std::path::{MAIN_SEPARATOR, PathBuf};

use hgm_agent::tools::{
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
fn tool_infos_match_the_python() {
    assert_eq!(bash_info(), case("bash_tool_info")["output"]);
    assert_eq!(editor_info(), case("editor_tool_info")["output"]);
    assert_eq!(tool_infos(), [bash_info(), editor_info()]);
}

#[test]
fn file_views_match_the_python() {
    for c in cases("tools") {
        let name = c["name"].as_str().unwrap();
        if name == "format_output_long_truncated" {
            let out = format_output(&"y".repeat(10010), "/repo/f.py");
            assert_eq!(out.len() as u64, c["output"]["len"].as_u64().unwrap());
            assert!(out.ends_with(c["output"]["tail"].as_str().unwrap()));
        } else if name.starts_with("format_output_") {
            let out = format_output(c["input"].as_str().unwrap(), "/repo/f.py");
            assert_eq!(out, c["output"].as_str().unwrap(), "{name}");
        }
    }
}

#[test]
fn truncation_counts_characters_and_tabs_reset_on_new_lines() {
    assert_eq!(maybe_truncate(&"é".repeat(10000)), "é".repeat(10000));
    assert_eq!(
        maybe_truncate(&"é".repeat(10001)),
        format!("{}\n<response clipped>", "é".repeat(10000))
    );
    assert_eq!(
        expand_tabs("ab\tc\r\td\n\te"),
        "ab      c\r        d\n        e"
    );
}

#[test]
fn editor_calls_match_the_python() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_str().unwrap();
    fs::write(tmp.path().join("f.txt"), "x\ny\n").unwrap();
    let tools = Tools::default();
    for c in cases("tools") {
        let name = c["name"].as_str().unwrap();
        if !name.starts_with("edit_tool_function_") || name.ends_with("_files_after") {
            continue;
        }
        let native = |text: &str| {
            text.replace("<TMP>/", &format!("{root}{MAIN_SEPARATOR}"))
                .replace("<TMP>", root)
        };
        let mut input = c["input"].clone();
        for value in input.as_object_mut().unwrap().values_mut() {
            if let Some(text) = value.as_str() {
                *value = json!(native(text));
            }
        }
        let expected = native(c["output"].as_str().unwrap());
        assert_eq!(tools.call("editor", &input), expected, "{name}");
    }
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

#[test]
fn directories_list_two_levels_without_hidden_entries() {
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
    assert_eq!(editor(Some("view"), &shown, None), want);
    assert_eq!(editor(Some("view"), &format!("{shown}/"), None), want);
    let hidden = root.join(".hidden").display().to_string();
    assert_eq!(
        editor(Some("view"), &hidden, None),
        format!(
            "Here's the files and directories up to 2 levels deep in {hidden}, excluding hidden items:\n\n"
        )
    );
}

#[cfg(unix)]
#[test]
fn unreadable_directories_and_files_are_reported() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok();
    let out = editor(Some("view"), tmp.path().to_str().unwrap(), None);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !readable {
        assert!(out.starts_with("Error listing directory: "), "{out}");
    }
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

#[test]
fn bad_tool_calls_are_reported() {
    let tools = Tools::default();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().to_str().unwrap();
    assert_eq!(tools.call("rm", &json!({})), "Error: Tool 'rm' not found");
    assert_eq!(
        tools.call("editor", &json!({"path": path, "command": null})),
        "Error: Unknown or unsupported command: None"
    );
    assert_eq!(
        tools.call("editor", &json!({"command": "view"})),
        "Error executing tool 'editor': missing required argument 'path'"
    );
    assert_eq!(
        tools.call("editor", &json!({"command": "view", "path": null})),
        "Error executing tool 'editor': argument 'path' must be a string, not null"
    );
    assert_eq!(
        tools.call("editor", &json!({"command": "view", "path": 3})),
        "Error executing tool 'editor': argument 'path' must be a string, not 3"
    );
    assert_eq!(
        tools.call("editor", &json!({"command": "view", "path": path, "x": 1})),
        "Error executing tool 'editor': unexpected argument 'x'"
    );
    assert_eq!(
        tools.call("bash", &json!([1])),
        "Error executing tool 'bash': arguments must be an object, not [1]"
    );
    assert_eq!(
        tools.call("bash", &Value::Null),
        "Error executing tool 'bash': missing required argument 'command'"
    );
    assert_eq!(
        tools.call("bash", &json!({"command": null})),
        "Error executing tool 'bash': argument 'command' must be a string, not null"
    );
}

#[test]
fn bash_errors_are_filtered_like_the_python() {
    for c in cases("tools") {
        let name = c["name"].as_str().unwrap();
        if name.starts_with("bash_filter_error_") {
            assert_eq!(
                filter_error(c["input"].as_str().unwrap()),
                c["output"].as_str().unwrap(),
                "{name}"
            );
        }
    }
    assert_eq!(filter_error("a\nInappropriate ioctl for device"), "a");
}
