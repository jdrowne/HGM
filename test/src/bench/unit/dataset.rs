use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use hgm_bench::dataset::{Error, Hub, instance_ids, read_parquet};
use serde_json::Value;
use std::ffi::OsString;

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
    assert_eq!(Value::Array(rows.clone()), expected_rows());
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

#[test]
fn load_downloads_each_split_file_once_and_then_reads_the_cache() {
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
    let hub = Hub {
        endpoint,
        cache_dir: tmp.path().to_path_buf(),
    };
    let first = hub.load(name, "test").unwrap();
    let second = hub.load(name, "test").unwrap();
    assert_eq!(Value::Array(first), expected_rows());
    assert_eq!(Value::Array(second), expected_rows());
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert!(
        tmp.path()
            .join("org/set/test/test-00000-of-00001.parquet")
            .is_file()
    );
}

#[test]
fn load_reports_a_split_without_files() {
    let (endpoint, _) = serve(vec![(
        "/api/datasets/org/set/tree/main/data".to_owned(),
        br#"[{"path":"data/train-00000-of-00001.parquet"}]"#.to_vec(),
    )]);
    let tmp = tempfile::tempdir().unwrap();
    let hub = Hub {
        endpoint,
        cache_dir: tmp.path().to_path_buf(),
    };
    assert!(matches!(
        hub.load("org/set", "test"),
        Err(Error::NoFiles { .. })
    ));
}

fn hub(endpoint: &str, cache_dir: &std::path::Path) -> Hub {
    Hub {
        endpoint: endpoint.to_owned(),
        cache_dir: cache_dir.to_path_buf(),
    }
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
fn files_that_are_missing_or_not_parquet_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_parquet(&tmp.path().join("missing")),
        Err(Error::Io(_))
    ));
    let junk = tmp.path().join("junk.parquet");
    fs::write(&junk, "not parquet").unwrap();
    assert!(matches!(read_parquet(&junk), Err(Error::Parquet(_))));
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
fn the_hub_comes_from_the_environment() {
    let vars = |pairs: &'static [(&'static str, &'static str)]| {
        move |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    };
    let default = Hub::from_vars(vars(&[]));
    assert_eq!(
        default,
        hub(
            "https://huggingface.co",
            &PathBuf::from("./.cache/hgm/datasets")
        )
    );
    let set = Hub::from_vars(vars(&[
        ("HGM_HF_ENDPOINT", "http://e"),
        ("HGM_DATA_DIR", "/d"),
    ]));
    assert_eq!(set, hub("http://e", &PathBuf::from("/d")));
    assert_eq!(
        Hub::from_vars(vars(&[("HOME", "/h")])).cache_dir,
        PathBuf::from("/h/.cache/hgm/datasets")
    );
    assert_eq!(
        Hub::from_vars(vars(&[("USERPROFILE", "/u")])).cache_dir,
        PathBuf::from("/u/.cache/hgm/datasets")
    );
    assert_eq!(Hub::from_env(), Hub::from_vars(|k| std::env::var_os(k)));
}

#[test]
fn errors_say_what_failed() {
    let cases = [
        (Error::from(std::io::Error::other("disk")), "disk"),
        (Error::Http("h".into()), "downloading the dataset: h"),
        (Error::Parquet("p".into()), "reading the dataset: p"),
        (
            Error::NoFiles {
                name: "n".into(),
                split: "s".into(),
            },
            "dataset n has no parquet files for split s",
        ),
    ];
    for (e, text) in cases {
        assert_eq!(e.to_string(), text);
    }
}

#[test]
fn http_failures_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let (endpoint, _) = serve(vec![]);
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Http(_))
    ));
    let (endpoint, _) = serve(vec![(LISTING_PATH.to_owned(), b"not json".to_vec())]);
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Http(_))
    ));
    let (endpoint, _) = serve(vec![(LISTING_PATH.to_owned(), LISTING.to_vec())]);
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Http(_))
    ));
    let (endpoint, _) = serve(vec![
        (LISTING_PATH.to_owned(), LISTING.to_vec()),
        (
            FILE_PATH.to_owned(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".to_vec(),
        ),
    ]);
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Http(_))
    ));
}

fn routes() -> Vec<(String, Vec<u8>)> {
    vec![
        (LISTING_PATH.to_owned(), LISTING.to_vec()),
        (FILE_PATH.to_owned(), fs::read(parquet_fixture()).unwrap()),
    ]
}

#[test]
fn an_unusable_cache_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("file"), "").unwrap();
    let (endpoint, _) = serve(routes());
    assert!(matches!(
        hub(&endpoint, &tmp.path().join("file")).load("org/set", "test"),
        Err(Error::Io(_))
    ));
    let dir = tmp.path().join("org/set/test");
    fs::create_dir_all(dir.join("test-0.partial")).unwrap();
    let (endpoint, _) = serve(routes());
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Io(_))
    ));
    fs::remove_dir(dir.join("test-0.partial")).unwrap();
    fs::create_dir_all(dir.join("test-0.parquet/inner")).unwrap();
    let (endpoint, _) = serve(routes());
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Io(_))
    ));
    fs::remove_dir_all(dir.join("test-0.parquet")).unwrap();
    fs::create_dir_all(dir.join("files.json")).unwrap();
    let (endpoint, _) = serve(routes());
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Io(_))
    ));
}

#[test]
fn a_stale_or_broken_listing_is_fetched_again_and_a_corrupt_file_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("org/set/test");
    fs::create_dir_all(&dir).unwrap();
    for listing in ["not json", "[]", r#"["gone.parquet"]"#] {
        fs::write(dir.join("files.json"), listing).unwrap();
        let (endpoint, seen) = serve(routes());
        assert_eq!(
            Value::Array(hub(&endpoint, tmp.path()).load("org/set", "test").unwrap()),
            expected_rows()
        );
        assert_eq!(seen.lock().unwrap().len(), 2, "{listing}");
    }
    fs::write(dir.join("test-0.parquet"), "junk").unwrap();
    let (endpoint, _) = serve(vec![]);
    assert!(matches!(
        hub(&endpoint, tmp.path()).load("org/set", "test"),
        Err(Error::Parquet(_))
    ));
}
