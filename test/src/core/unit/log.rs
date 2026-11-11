use rustyhgm_core::log::{Level, Log};

fn written_lines() -> Vec<String> {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("run.log");
    let log = Log::to_file(&path).unwrap();
    log.info("one");
    log.warning("two");
    log.error("three");
    Log::discard().error("dropped");
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines().map(str::to_owned).collect()
}

#[test]
fn a_log_file_gets_one_line_per_kept_message() {
    assert_eq!(written_lines().len(), 3);
}

#[test]
fn an_info_line_ends_with_the_info_name() {
    let line = &written_lines()[0];
    assert!(line.ends_with(" - INFO - one"), "{line}");
}

#[test]
fn a_warning_line_ends_with_the_warning_name() {
    let line = &written_lines()[1];
    assert!(line.ends_with(" - WARNING - two"), "{line}");
}

#[test]
fn an_error_line_ends_with_the_error_name() {
    let line = &written_lines()[2];
    assert!(line.ends_with(" - ERROR - three"), "{line}");
}

#[test]
fn a_thread_without_a_name_is_shown_by_its_id() {
    let line = std::thread::spawn(|| Log::line(Level::Info, "m"))
        .join()
        .unwrap();
    assert!(line.contains(" - ThreadId("), "{line}");
}

#[test]
fn a_named_thread_is_shown_by_its_name() {
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
