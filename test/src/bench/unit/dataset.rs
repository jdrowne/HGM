use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rustyhgm_bench::dataset::{Error, Hub, instance_ids, read_parquet};
use serde_json::Value;

use crate::common::cases;

fn parquet_fixture() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../../test/src/bench/unit/fixtures/rows.parquet")
}

fn expected_rows() -> Value {
    cases("dataset_rows")[0]["output"].clone()
}

#[test]
fn parquet_rows_match_the_datasets_package() {
    let rows = read_parquet(&parquet_fixture()).unwrap();
    assert_eq!(Value::Array(rows), expected_rows());
}

#[test]
fn instance_ids_are_read_from_the_rows() {
    let rows = read_parquet(&parquet_fixture()).unwrap();
    assert_eq!(
        instance_ids(&rows),
        [
            "astropy__astropy-12907",
            "django__django-11099",
            "sympy__sympy-20590"
        ]
    );
}

fn serve(routes: Vec<(String, Vec<u8>)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            reader.read_line(&mut line).unwrap();
            let mut header = String::new();
            while reader.read_line(&mut header).unwrap() > 2 {
                header.clear();
            }
            let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
            log.lock().unwrap().push(path.clone());
            let found = routes.iter().find(|(p, _)| *p == path);
            if let Some((_, raw)) = found.filter(|(_, b)| b.starts_with(b"HTTP/1.1 ")) {
                stream.write_all(raw).unwrap();
                continue;
            }
            let (status, body) = found.map_or(("404 Not Found", Vec::new()), |(_, b)| {
                ("200 OK", b.clone())
            });
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (base, seen)
}

struct Loaded {
    tmp: tempfile::TempDir,
    first: Vec<Value>,
    second: Vec<Value>,
    seen: Arc<Mutex<Vec<String>>>,
}

fn loaded_twice() -> Loaded {
    let name = "org/set";
    let listing = r#"[{"path":"data/train-00000-of-00001.parquet"},{"path":"data/test-00000-of-00001.parquet"},{"path":"data/test.json"}]"#;
    let (endpoint, seen) = serve(vec![
        (
            format!("/api/datasets/{name}/tree/main/data"),
            listing.as_bytes().to_vec(),
        ),
        (
            format!("/datasets/{name}/resolve/main/data/test-00000-of-00001.parquet"),
            std::fs::read(parquet_fixture()).unwrap(),
        ),
    ]);
    let tmp = tempfile::tempdir().unwrap();
    let hub = hub(&endpoint, tmp.path());
    let first = hub.load(name, "test").unwrap();
    let second = hub.load(name, "test").unwrap();
    Loaded {
        tmp,
        first,
        second,
        seen,
    }
}

#[test]
fn load_reads_the_split_rows() {
    assert_eq!(Value::Array(loaded_twice().first), expected_rows());
}

#[test]
fn load_reads_the_same_rows_from_the_cache() {
    assert_eq!(Value::Array(loaded_twice().second), expected_rows());
}

#[test]
fn load_downloads_each_split_file_once() {
    assert_eq!(loaded_twice().seen.lock().unwrap().len(), 2);
}

#[test]
fn load_caches_the_split_file() {
    assert!(
        loaded_twice()
            .tmp
            .path()
            .join("org/set/test/test-00000-of-00001.parquet")
            .is_file()
    );
}

fn hub(endpoint: &str, cache_dir: &std::path::Path) -> Hub {
    Hub {
        endpoint: endpoint.to_owned(),
        cache_dir: cache_dir.to_path_buf(),
    }
}

#[test]
fn load_reports_a_split_without_files() {
    let (endpoint, _) = serve(vec![(
        "/api/datasets/org/set/tree/main/data".to_owned(),
        br#"[{"path":"data/train-00000-of-00001.parquet"}]"#.to_vec(),
    )]);
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::NoFiles { .. })
    ));
}

const LISTING_PATH: &str = "/api/datasets/org/set/tree/main/data";
const FILE_PATH: &str = "/datasets/org/set/resolve/main/data/test-0.parquet";
const LISTING: &[u8] = br#"[{"path":"data/test-0.parquet"}]"#;

#[test]
fn every_column_type_reads_like_the_datasets_package() {
    let path = parquet_fixture().with_file_name("types.parquet");
    let rows = read_parquet(&path).unwrap();
    assert_eq!(Value::Array(rows), cases("dataset_types")[0]["output"]);
}

#[test]
fn a_missing_file_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_parquet(&tmp.path().join("missing")),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_file_that_is_not_parquet_is_a_parquet_error() {
    let tmp = tempfile::tempdir().unwrap();
    let junk = tmp.path().join("junk.parquet");
    fs::write(&junk, "not parquet").unwrap();
    assert!(matches!(read_parquet(&junk), Err(Error::Parquet(_))));
}

#[test]
fn a_corrupt_parquet_file_is_a_parquet_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = fs::read(parquet_fixture()).unwrap();
    let end = bytes.len() / 2;
    for b in &mut bytes[4..end] {
        *b = 0xa5;
    }
    let corrupt = tmp.path().join("corrupt.parquet");
    fs::write(&corrupt, bytes).unwrap();
    assert!(matches!(read_parquet(&corrupt), Err(Error::Parquet(_))));
}

#[test]
fn errors_say_what_failed() {
    let actual: Vec<String> = [
        Error::from(std::io::Error::other("disk")),
        Error::Http("h".into()),
        Error::Parquet("p".into()),
        Error::NoFiles {
            name: "n".into(),
            split: "s".into(),
        },
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    assert_eq!(
        actual,
        [
            "disk",
            "downloading the dataset: h",
            "reading the dataset: p",
            "dataset n has no parquet files for split s",
        ]
    );
}

fn load_with(routes: Vec<(String, Vec<u8>)>) -> Result<Vec<Value>, Error> {
    let tmp = tempfile::tempdir().unwrap();
    let (endpoint, _) = serve(routes);
    hub(&endpoint, tmp.path()).load("org/set", "test")
}

#[test]
fn a_missing_listing_is_an_http_error() {
    assert!(matches!(load_with(vec![]), Err(Error::Http(_))));
}

#[test]
fn a_listing_that_is_not_json_is_an_http_error() {
    assert!(matches!(
        load_with(vec![(LISTING_PATH.to_owned(), b"not json".to_vec())]),
        Err(Error::Http(_))
    ));
}

#[test]
fn a_missing_split_file_is_an_http_error() {
    assert!(matches!(
        load_with(vec![(LISTING_PATH.to_owned(), LISTING.to_vec())]),
        Err(Error::Http(_))
    ));
}

#[test]
fn a_cut_short_download_is_an_http_error() {
    assert!(matches!(
        load_with(vec![
            (LISTING_PATH.to_owned(), LISTING.to_vec()),
            (
                FILE_PATH.to_owned(),
                b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort"
                    .to_vec(),
            ),
        ]),
        Err(Error::Http(_))
    ));
}

fn routes() -> Vec<(String, Vec<u8>)> {
    vec![
        (LISTING_PATH.to_owned(), LISTING.to_vec()),
        (FILE_PATH.to_owned(), fs::read(parquet_fixture()).unwrap()),
    ]
}

fn blocked_by(entry: &str) -> Result<Vec<Value>, Error> {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("org/set/test").join(entry)).unwrap();
    let (endpoint, _) = serve(routes());
    hub(&endpoint, tmp.path()).load("org/set", "test")
}

#[test]
fn a_cache_that_is_a_file_is_an_io_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("file"), "").unwrap();
    let (endpoint, _) = serve(routes());
    assert!(matches!(
        hub(&endpoint, &tmp.path().join("file")).load("org/set", "test"),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_blocked_partial_download_is_an_io_error() {
    assert!(matches!(blocked_by("test-0.partial"), Err(Error::Io(_))));
}

#[test]
fn a_blocked_split_file_is_an_io_error() {
    assert!(matches!(
        blocked_by("test-0.parquet/inner"),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_blocked_listing_cache_is_an_io_error() {
    assert!(matches!(blocked_by("files.json"), Err(Error::Io(_))));
}

fn stale_requests(listing: &str) -> usize {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("org/set/test");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("files.json"), listing).unwrap();
    let (endpoint, seen) = serve(routes());
    let rows = hub(&endpoint, tmp.path()).load("org/set", "test").unwrap();
    if Value::Array(rows) != expected_rows() {
        return usize::MAX;
    }
    seen.lock().unwrap().len()
}

#[test]
fn a_stale_or_broken_listing_is_fetched_again() {
    let counts: Vec<usize> = ["not json", "[]", r#"["gone.parquet"]"#]
        .into_iter()
        .map(stale_requests)
        .collect();
    assert_eq!(counts, [2, 2, 2]);
}

#[test]
fn a_corrupt_cached_file_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (endpoint, _) = serve(routes());
    hub(&endpoint, tmp.path()).load("org/set", "test").unwrap();
    fs::write(tmp.path().join("org/set/test/test-0.parquet"), "junk").unwrap();
    let (endpoint, _) = serve(vec![]);
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Parquet(_))
    ));
}
