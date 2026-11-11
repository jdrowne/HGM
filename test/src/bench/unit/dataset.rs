use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use hgm_bench::dataset::{Error, Hub, instance_ids, read_parquet};
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
