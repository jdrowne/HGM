mod polyglot;
mod swe;

use std::fs;
use std::path::{Path, PathBuf};

use rustyhgm_coding_agent::llm::{Provider, Service};

pub fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

pub fn service(root: &Path) -> Service {
    fs::create_dir_all(root).unwrap();
    let key = root.join("key");
    fs::write(&key, "secret\n").unwrap();
    Service {
        provider: Provider::Vllm,
        base_url: "http://h/v1".into(),
        api_key_file: key,
    }
}

pub fn dangling(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).unwrap();
}

pub fn once(name: &str, build: impl FnOnce(&Path)) -> PathBuf {
    let run = std::env::var("NEXTEST_RUN_ID").unwrap_or_else(|_| std::process::id().to_string());
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{run}"));
    fs::create_dir_all(&dir).unwrap();
    let lock = fs::File::create(dir.join("lock")).unwrap();
    lock.lock().unwrap();
    let done = dir.join("done");
    if let Ok(root) = fs::read_to_string(&done) {
        return PathBuf::from(root);
    }
    let root = tempfile::Builder::new()
        .prefix("attempt")
        .tempdir_in(&dir)
        .unwrap()
        .keep();
    build(&root);
    fs::write(&done, root.to_string_lossy().as_bytes()).unwrap();
    root
}

#[test]
fn crate_is_linked() {
    assert_eq!(rustyhgm_bench::CRATE_NAME, "rustyhgm-bench");
}
