use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(windows)]
const DEVNULL: &str = "NUL";
#[cfg(not(windows))]
const DEVNULL: &str = "/dev/null";

fn stdout(git: &str, dir: &Path, args: &[&str]) -> io::Result<String> {
    Command::new(git)
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

fn combined(git: &str, dir: &Path, args: &[&str]) -> io::Result<String> {
    let mut bytes = Vec::new();
    io::pipe().and_then(|(mut reader, writer)| {
        writer.try_clone().and_then(|out| {
            let mut cmd = Command::new(git);
            cmd.arg("-C")
                .arg(dir)
                .args(args)
                .current_dir(dir)
                .stdin(Stdio::null())
                .stdout(out)
                .stderr(writer);
            let child = cmd.spawn();
            drop(cmd);
            child.and_then(|mut child| reader.read_to_end(&mut bytes).and_then(|_| child.wait()))
        })
    })?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn diff_versus_commit_with(git: &str, dir: &Path, commit: &str) -> io::Result<String> {
    stdout(git, dir, &["diff", commit])
        .and_then(|diff| {
            stdout(git, dir, &["ls-files", "--others", "--exclude-standard"])
                .map(|untracked| (diff, untracked))
        })
        .and_then(|(mut diff, untracked)| {
            untracked
                .lines()
                .try_for_each(|file| {
                    combined(git, dir, &["diff", "--no-index", DEVNULL, file])
                        .map(|text| diff.push_str(&text))
                })
                .map(|()| diff)
        })
}

pub fn diff_versus_commit(dir: &Path, commit: &str) -> io::Result<String> {
    diff_versus_commit_with("git", dir, commit)
}
