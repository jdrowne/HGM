use std::fs;
use std::io;
use std::path::Path;

use include_dir::{Dir, DirEntry, include_dir};

static AGENT: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../agent");
static AGENT_TESTS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../test/src/agent");

const FILES: [(&str, &[u8]); 5] = [
    ("Cargo.toml", include_bytes!("../../Cargo.toml")),
    (
        "rust-toolchain.toml",
        include_bytes!("../../rust-toolchain.toml"),
    ),
    ("LICENSE.txt", include_bytes!("../../LICENSE.txt")),
    ("Dockerfile", include_bytes!("../../Dockerfile")),
    (".dockerignore", include_bytes!("../../.dockerignore")),
];

fn collect(dir: &'static Dir<'static>, prefix: &str, files: &mut Vec<(String, &'static [u8])>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(sub) => collect(sub, prefix, files),
            DirEntry::File(file) => files.push((
                format!(
                    "{prefix}/{}",
                    file.path().to_string_lossy().replace('\\', "/")
                ),
                file.contents(),
            )),
        }
    }
}

pub fn files() -> Vec<(String, &'static [u8])> {
    let mut files: Vec<(String, &'static [u8])> = FILES
        .iter()
        .map(|(path, contents)| ((*path).to_owned(), *contents))
        .collect();
    collect(&AGENT, "src/agent", &mut files);
    collect(&AGENT_TESTS, "test/src/agent", &mut files);
    files
}

pub fn write(dest: &Path) -> io::Result<()> {
    for (rel, contents) in files() {
        let path = dest.join(rel);
        fs::create_dir_all(path.parent().unwrap_or(dest))?;
        fs::write(&path, contents)?;
    }
    Ok(())
}
