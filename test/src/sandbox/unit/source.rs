use std::fs;
use std::path::Path;

use hgm_core::log::Log;
use hgm_core::pyjson;
use hgm_sandbox::source::{copy_filtered, image_name_for, initial_metadata, prepare_initial_agent};

#[test]
fn initial_metadata_matches_json_dump() {
    assert_eq!(
        pyjson::dumps(&initial_metadata(), None),
        "{\"run_id\": \"initial\", \"overall_performance\": {\"accuracy_score\": 0, \
         \"total_resolved_instances\": 0, \"total_submitted_instances\": 0, \"files\": [], \
         \"total_unresolved_ids\": [], \"total_emptypatch_ids\": [], \"total_resolved_ids\": [], \
         \"total_submitted_ids\": []}}"
    );
}

#[test]
fn image_is_named_after_the_parent_folder() {
    assert_eq!(
        image_name_for(Path::new("/x/initial_swe/src")).as_deref(),
        Some("initial_swe")
    );
}

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("repo");
    fs::create_dir_all(src.join("pkg/__pycache__")).unwrap();
    fs::create_dir_all(src.join("logs")).unwrap();
    fs::write(
        src.join(".dockerignore"),
        "# c\nlogs/\n**/__pycache__/\n.dockerignore\n",
    )
    .unwrap();
    fs::write(src.join("Dockerfile"), "FROM scratch\n").unwrap();
    fs::write(src.join("main.py"), "print(1)\n").unwrap();
    fs::write(src.join("pkg/mod.py"), "x = 1\n").unwrap();
    fs::write(src.join("pkg/__pycache__/mod.pyc"), "").unwrap();
    fs::write(src.join("logs/run.log"), "").unwrap();
    dir
}

#[test]
fn copy_skips_ignored_files() {
    let dir = tree();
    let dest = dir.path().join("out/initial/src");
    let report = copy_filtered(&dir.path().join("repo"), &dest).unwrap();
    assert_eq!(report.patterns, 3);
    assert_eq!(report.copied, ["Dockerfile", "main.py", "pkg/mod.py"]);
    assert_eq!(
        fs::read_to_string(dest.join("pkg/mod.py")).unwrap(),
        "x = 1\n"
    );
    assert!(!dest.join("logs").exists());
}

#[test]
fn prepare_writes_metadata_once() {
    let dir = tree();
    let dest = dir.path().join("out/initial/src");
    let meta = dir.path().join("out/initial/metadata.json");
    prepare_initial_agent(&dir.path().join("repo"), &dest, None, &Log::discard()).unwrap();
    assert_eq!(
        fs::read_to_string(&meta).unwrap(),
        pyjson::dumps(&initial_metadata(), None)
    );
    fs::write(&meta, "{}").unwrap();
    prepare_initial_agent(&dir.path().join("repo"), &dest, None, &Log::discard()).unwrap();
    assert_eq!(fs::read_to_string(&meta).unwrap(), "{}");
}
