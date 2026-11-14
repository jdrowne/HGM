const INTRO: &str = "# Interface\n\nThe tests call the solution through the declarations below. Declare each one with exactly this name, these parameters and this return type. Everything else is yours to design.\n\n";

const INDENT: &str = "    ";

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn squeeze(text: &str) -> String {
    let spaced = text.split_whitespace().collect::<Vec<_>>().join(" ");
    spaced.replace("( ", "(").replace(" )", ")")
}

fn literal_end(text: &str) -> usize {
    let mut escaped = false;
    let closing = text.bytes().enumerate().skip(1).find(|&(_, b)| {
        let closes = !escaped && Some(b) == text.bytes().next();
        escaped = !escaped && b == b'\\';
        closes
    });
    closing.map_or(text.len(), |(i, _)| i + 1)
}

fn token(text: &str) -> (usize, &str) {
    if text.starts_with("//") {
        (text.find('\n').unwrap_or(text.len()), "")
    } else if text.starts_with("/*") {
        (text.find("*/").map_or(text.len(), |i| i + 2), " ")
    } else if text.starts_with('/') {
        (1, "/")
    } else {
        let end = literal_end(text);
        (end, &text[..end])
    }
}

fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['"', '\'', '/']) {
        let (used, kept) = token(&rest[i..]);
        out.push_str(&rest[..i]);
        out.push_str(kept);
        rest = &rest[i + used..];
    }
    out + rest
}

fn without_directives(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn code(text: &str) -> impl Iterator<Item = (usize, u8)> + '_ {
    let mut resume = 0;
    text.bytes().enumerate().filter(move |&(i, b)| {
        if i >= resume && matches!(b, b'"' | b'\'') {
            resume = i + literal_end(&text[i..]);
        }
        i >= resume
    })
}

fn nesting(depth: &mut usize, b: u8, open: u8, close: u8) {
    if b == open {
        *depth += 1;
    } else if b == close {
        *depth = depth.saturating_sub(1);
    }
}

fn lone_colon(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0;
    code(text)
        .find(|&(i, b)| {
            nesting(&mut depth, b, b'(', b')');
            depth == 0 && b == b':' && bytes.get(i + 1) != Some(&b':') && !text[..i].ends_with(':')
        })
        .map(|(i, _)| i)
}

fn word_at(text: &str, word: &str) -> Option<usize> {
    text.match_indices(word)
        .map(|(i, _)| i)
        .find(|&i| !text[..i].ends_with(is_word) && !text[i + word.len()..].starts_with(is_word))
}

fn after_templates(head: &str) -> &str {
    let mut rest = head;
    while let Some(list) = rest
        .strip_prefix("template")
        .map(str::trim_start)
        .filter(|list| list.starts_with('<'))
    {
        let mut depth = 0;
        let end = list.bytes().position(|b| {
            nesting(&mut depth, b, b'<', b'>');
            depth == 0
        });
        rest = end.map_or("", |i| list[i + 1..].trim_start());
    }
    rest
}

enum Kind {
    Function,
    Enum,
    Namespace,
    Class(bool),
    Value,
}

fn kind(head: &str) -> Kind {
    let plain = after_templates(head);
    let has = |word: &str| word_at(plain, word).is_some();
    if plain.contains('(') {
        Kind::Function
    } else if has("enum") {
        Kind::Enum
    } else if has("namespace") {
        Kind::Namespace
    } else if has("class") {
        Kind::Class(false)
    } else if has("struct") || has("union") {
        Kind::Class(true)
    } else {
        Kind::Value
    }
}

fn qualified(signature: &str) -> bool {
    let name = signature.split('(').next().unwrap_or_default().trim_end();
    let owner = match word_at(name, "operator") {
        Some(i) => &name[..i],
        None => name.trim_end_matches(|c| is_word(c) || c == '~'),
    };
    owner.trim_end().ends_with("::")
}

fn signature(head: &str) -> &str {
    lone_colon(head).map_or(head, |i| head[..i].trim_end())
}

fn access(head: &str) -> Option<(bool, &str)> {
    ["public", "private", "protected"]
        .into_iter()
        .find_map(|label| {
            let rest = head.strip_prefix(label)?.trim_start().strip_prefix(':')?;
            (!rest.starts_with(':')).then(|| (label == "public", rest.trim_start()))
        })
}

fn declaration(kind: &Kind, head: &str, body: &str, top: bool) -> Option<String> {
    match kind {
        Kind::Enum => Some(format!("{head} {{ {} }};", squeeze(body))),
        Kind::Function => {
            let signature = signature(head);
            (!(top && qualified(signature))).then(|| format!("{signature};"))
        }
        _ => {
            let name = head.trim_end_matches('=').trim_end();
            (!name.is_empty()).then(|| format!("{name};"))
        }
    }
}

struct Walk<'a> {
    text: &'a str,
    lines: Vec<String>,
    depth: usize,
}

impl<'a> Walk<'a> {
    fn advance(&mut self, end: Option<usize>) -> &'a str {
        let text = self.text;
        let (taken, rest) = match end {
            Some(i) => (&text[..i], &text[i + 1..]),
            None => (text, ""),
        };
        self.text = rest;
        taken
    }

    fn head(&mut self) -> (String, Option<u8>) {
        let mut depth = 0;
        let end = code(self.text).find(|&(_, b)| {
            nesting(&mut depth, b, b'(', b')');
            depth == 0 && matches!(b, b';' | b'{' | b'}')
        });
        let taken = self.advance(end.map(|(i, _)| i));
        (squeeze(taken), end.map(|(_, b)| b))
    }

    fn block(&mut self) -> &'a str {
        let mut depth = 1;
        let end = code(self.text).find(|&(_, b)| {
            nesting(&mut depth, b, b'{', b'}');
            depth == 0
        });
        self.advance(end.map(|(i, _)| i))
    }

    fn tail(&mut self) {
        while self.text.trim_start().starts_with([',', '{']) {
            if self.head().1 == Some(b'{') {
                self.block();
            }
        }
    }

    fn emit(&mut self, line: &str) {
        self.lines
            .push(format!("{}{line}", INDENT.repeat(self.depth)));
    }

    fn nested(&mut self, head: &str, visible: bool, top: bool) {
        self.emit(&format!("{head} {{"));
        if !visible {
            self.emit("public:");
        }
        self.depth += 1;
        self.members(visible, top);
        self.depth -= 1;
        self.emit(if top { "}" } else { "};" });
    }

    fn dropped(&mut self, kind: &Kind, head: &str, shown: bool, top: bool) {
        let body = self.block();
        self.tail();
        if let Some(line) = declaration(kind, head, body, top).filter(|_| shown) {
            self.emit(&line);
        }
    }

    fn members(&mut self, mut visible: bool, top: bool) {
        loop {
            let (text, end) = self.head();
            let mut head = text.as_str();
            while let Some((public, rest)) = access(head) {
                visible = public;
                head = rest;
            }
            match (end, kind(head)) {
                (Some(b';'), _) if visible && !head.is_empty() => self.emit(&format!("{head};")),
                (Some(b';'), _) => {}
                (Some(b'{'), Kind::Namespace) => self.nested(head, true, true),
                (Some(b'{'), Kind::Class(open)) if visible => self.nested(head, open, false),
                (Some(b'{'), kind) => self.dropped(&kind, head, visible, top),
                _ => return,
            }
        }
    }
}

pub fn declarations(header: &str) -> String {
    let text = without_directives(&without_comments(header));
    let mut walk = Walk {
        text: &text,
        lines: Vec::new(),
        depth: 0,
    };
    walk.members(true, true);
    walk.lines.join("\n")
}

pub fn section<S: AsRef<str>>(headers: &[S]) -> String {
    let declared: Vec<String> = headers
        .iter()
        .map(|header| declarations(header.as_ref()))
        .filter(|declared| !declared.is_empty())
        .collect();
    if declared.is_empty() {
        String::new()
    } else {
        format!("{INTRO}```cpp\n{}\n```\n\n", declared.join("\n"))
    }
}
