use hgm_core::log::{Level, Log};

#[test]
fn each_level_writes_its_python_name() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("run.log");
    let log = Log::to_file(&path).unwrap();
    log.info("one");
    log.warning("two");
    log.error("three");
    Log::discard().error("dropped");
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    for (line, (level, message)) in
        lines
            .iter()
            .zip([("INFO", "one"), ("WARNING", "two"), ("ERROR", "three")])
    {
        assert!(line.ends_with(&format!(" - {level} - {message}")), "{line}");
    }
}

#[test]
fn a_thread_without_a_name_is_shown_by_its_id() {
    let line = std::thread::spawn(|| Log::line(Level::Info, "m"))
        .join()
        .unwrap();
    assert!(line.contains(" - ThreadId("), "{line}");
    let named = std::thread::Builder::new()
        .name("worker".into())
        .spawn(|| Log::line(Level::Warning, "m"))
        .unwrap()
        .join()
        .unwrap();
    assert!(named.contains(" - worker - WARNING - m\n"), "{named}");
}

#[test]
fn a_log_in_a_missing_directory_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Log::to_file(&tmp.path().join("no/such/dir/run.log")).is_err());
}
