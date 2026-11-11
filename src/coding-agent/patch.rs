pub fn splitlines(text: &str, keepends: bool) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end = match c {
            '\r' => match chars.peek() {
                Some(&(j, '\n')) => {
                    chars.next();
                    j + 1
                }
                _ => i + 1,
            },
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => i + c.len_utf8(),
            _ => continue,
        };
        lines.push(if keepends {
            &text[start..end]
        } else {
            &text[start..i]
        });
        start = end;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

pub fn filter_patch_by_files<S: AsRef<str>>(patch: &str, target_files: &[S]) -> String {
    let mut out = Vec::new();
    let mut include = false;
    for line in splitlines(patch, false) {
        if line.starts_with("diff --git") {
            include = target_files.iter().any(|t| {
                let t = t.as_ref();
                line.contains(&format!("a/{t}")) && line.contains(&format!("b/{t}"))
            });
        }
        if include {
            out.push(line);
        }
    }
    out.join("\n")
}

pub fn remove_patch_by_files(patch: &str, keyword: &str) -> String {
    let keyword = keyword.to_lowercase();
    let mut out = Vec::new();
    let mut include = true;
    for line in splitlines(patch, false) {
        if line.starts_with("diff --git") {
            include = !line.to_lowercase().contains(&keyword);
        }
        if include {
            out.push(line);
        }
    }
    out.join("\n")
}

pub fn remove_patches_to_tests(patch: &str) -> String {
    let mut out = String::new();
    let mut is_tests = false;
    for line in splitlines(patch, true) {
        if line.starts_with("diff --git a/") {
            let to = line.split_whitespace().last().unwrap_or("");
            is_tests = to.starts_with("b/")
                && ["/test/", "/tests/", "/testing/", "/test_", "/tox.ini"]
                    .iter()
                    .any(|m| to.contains(m));
        }
        if !is_tests {
            out.push_str(line);
        }
    }
    out
}
