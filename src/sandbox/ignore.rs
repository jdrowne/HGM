use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use regex::Regex;

use crate::error::Error;

pub const DOCKERIGNORE_FILE: &str = ".dockerignore";

pub fn parse_dockerignore(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("<!--"))
        .map(str::to_owned)
        .collect()
}

pub fn read_dockerignore(path: &Path) -> io::Result<Vec<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(parse_dockerignore(&text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

#[derive(Debug)]
struct Pattern {
    regex: Regex,
    include: bool,
}

#[derive(Debug, Default)]
pub struct Spec {
    patterns: Vec<Pattern>,
}

impl Spec {
    pub fn new<S: AsRef<str>>(lines: &[S]) -> Result<Self, Error> {
        let mut patterns = Vec::new();
        for line in lines {
            if let Some((regex, include)) = pattern_to_regex(line.as_ref())? {
                let regex =
                    Regex::new(&regex).map_err(|_| Error::Pattern(line.as_ref().to_owned()))?;
                patterns.push(Pattern { regex, include });
            }
        }
        Ok(Spec { patterns })
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn matches(&self, path: &str) -> bool {
        let path = path.trim_start_matches('/');
        let mut result = false;
        for p in &self.patterns {
            if p.regex.is_match(path) {
                result = p.include;
            }
        }
        result
    }

    pub fn walk(&self, root: &Path) -> io::Result<BTreeSet<String>> {
        let mut files = BTreeSet::new();
        self.walk_dir(root, "", &mut files)?;
        Ok(files)
    }

    fn walk_dir(&self, dir: &Path, prefix: &str, files: &mut BTreeSet<String>) -> io::Result<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<io::Result<_>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = format!("{prefix}{name}");
            let path = entry.path();
            let is_dir = fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            if is_dir {
                if self.matches(&format!("{rel}/")) {
                    continue;
                }
                if !entry.file_type()?.is_symlink() {
                    self.walk_dir(&path, &format!("{rel}/"), files)?;
                }
            } else if !self.matches(&rel) {
                files.insert(rel);
            }
        }
        Ok(())
    }
}

const DIR_MARK_OPT: &str = "(?:/|$)";

fn pattern_to_regex(pattern: &str) -> Result<Option<(String, bool)>, Error> {
    let invalid = || Error::Pattern(pattern.to_owned());
    let mut p = if pattern.ends_with("\\ ") {
        pattern
    } else {
        pattern.trim_end()
    };
    if p.is_empty() || p.starts_with('#') || p == "/" {
        return Ok(None);
    }
    let include = match p.strip_prefix('!') {
        Some(rest) => {
            p = rest;
            false
        }
        None => true,
    };
    let mut segs: Vec<String> = p.split('/').map(str::to_owned).collect();
    let is_dir_pattern = segs.last().is_some_and(String::is_empty);
    if segs[0].is_empty() {
        segs.remove(0);
    } else if (segs.len() == 1 || (segs.len() == 2 && segs[1].is_empty())) && segs[0] != "**" {
        segs.insert(0, "**".to_owned());
    }
    if segs.is_empty() {
        return Err(invalid());
    }
    if let Some(last) = segs.last_mut()
        && last.is_empty()
    {
        *last = "**".to_owned();
    }
    let mut i = segs.len() - 1;
    while i > 0 {
        if segs[i - 1] == "**" && segs[i] == "**" {
            segs.remove(i);
        }
        i -= 1;
    }
    let s: Vec<&str> = segs.iter().map(String::as_str).collect();
    let regex = match s.as_slice() {
        ["**"] if is_dir_pattern => "/".to_owned(),
        ["**"] | ["**", "*"] => ".".to_owned(),
        ["**", "*", "**"] => "/".to_owned(),
        _ => match translate_segments(&s) {
            Ok(r) => r,
            Err(SegError::Range) => return Ok(None),
            Err(SegError::Escape) => return Err(invalid()),
        },
    };
    Ok(Some((regex, include)))
}

enum SegError {
    Range,
    Escape,
}

fn translate_segments(segs: &[&str]) -> Result<String, SegError> {
    let mut out = String::new();
    let mut need_slash = false;
    let end = segs.len() - 1;
    for (i, seg) in segs.iter().enumerate() {
        if *seg == "**" {
            if i == 0 {
                out.push_str("^(?:.+/)?");
            } else if i < end {
                out.push_str("(?:/.+)?");
                need_slash = true;
            } else {
                out.push('/');
            }
        } else {
            if i == 0 {
                out.push('^');
            }
            if need_slash {
                out.push('/');
            }
            if *seg == "*" {
                out.push_str("[^/]+");
            } else {
                out.push_str(&translate_glob(seg)?);
            }
            if i == end {
                out.push_str(DIR_MARK_OPT);
            }
            need_slash = true;
        }
    }
    Ok(out)
}

fn translate_glob(pattern: &str) -> Result<String, SegError> {
    let chars: Vec<char> = pattern.chars().collect();
    let end = chars.len();
    let mut regex = String::new();
    let mut escape = false;
    let mut i = 0;
    while i < end {
        let c = chars[i];
        i += 1;
        if escape {
            escape = false;
            regex.push_str(&regex::escape(&c.to_string()));
        } else if c == '\\' {
            escape = true;
        } else if c == '*' {
            regex.push_str("[^/]*");
        } else if c == '?' {
            regex.push_str("[^/]");
        } else if c == '[' {
            let mut j = i;
            if j < end && (chars[j] == '!' || chars[j] == '^') {
                j += 1;
            }
            if j < end && chars[j] == ']' {
                j += 1;
            }
            while j < end && chars[j] != ']' {
                j += 1;
            }
            if j >= end {
                return Err(SegError::Range);
            }
            j += 1;
            let mut expr = String::from("[");
            if chars[i] == '!' || chars[i] == '^' {
                expr.push('^');
                i += 1;
            }
            let body: String = chars[i..j].iter().collect();
            expr.push_str(&body.replace('\\', "\\\\"));
            regex.push_str(&expr);
            i = j;
        } else {
            regex.push_str(&regex::escape(&c.to_string()));
        }
    }
    if escape {
        return Err(SegError::Escape);
    }
    Ok(regex)
}
