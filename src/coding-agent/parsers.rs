use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

pub type Report = Map<String, Value>;

pub type Parser = fn(&str) -> Option<Report>;

const STATUSES: [&str; 5] = ["FAILED", "PASSED", "SKIPPED", "ERROR", "XFAIL"];

fn set(report: &mut Report, test: &str, status: &str) {
    report.insert(test.to_owned(), Value::String(status.to_owned()));
}

fn starts_with_status(line: &str) -> bool {
    STATUSES.iter().any(|s| line.starts_with(s))
}

fn status_line(line: &str) -> Option<(String, String)> {
    if !starts_with_status(line) {
        return None;
    }
    let line = if line.starts_with("FAILED") {
        line.replace(" - ", " ")
    } else {
        line.to_owned()
    };
    let mut words = line.split_whitespace();
    let (Some(status), Some(test)) = (words.next(), words.next()) else {
        return None;
    };
    Some((status.to_owned(), test.to_owned()))
}

pub fn parse_log_pytest(log: &str) -> Option<Report> {
    let mut report = Report::new();
    for (status, test) in log.split('\n').filter_map(status_line) {
        set(&mut report, &test, &status);
    }
    Some(report)
}

static OPTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(.*?)\[(.*)\]").unwrap());

pub fn parse_log_pytest_options(log: &str) -> Option<Report> {
    let mut report = Report::new();
    for (status, test) in log.split('\n').filter_map(status_line) {
        let name = match OPTION.captures(&test) {
            Some(c) => {
                let mut option = c[2].to_owned();
                if option.starts_with('/') && !option.starts_with("//") && !option.contains('*') {
                    option = format!("/{}", option.rsplit('/').next().unwrap_or_default());
                }
                format!("{}[{option}]", &c[1])
            }
            None => test,
        };
        set(&mut report, &name, &status);
    }
    Some(report)
}

static DJANGO_PATTERNS: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        r"(?m)^(.*?)\s\.\.\.\sTesting against Django installed in ((?s:.*?)) silenced\)\.\nok$",
        r"(?m)^(.*?)\s\.\.\.\sInternal Server Error: /(.*)/\nok$",
        r"(?m)^(.*?)\s\.\.\.\sSystem check identified no issues \(0 silenced\)\nok$",
    ]
    .map(|p| Regex::new(p).unwrap())
});

fn second_word(line: &str) -> Option<&str> {
    line.split_whitespace().nth(1)
}

fn django_line(line: &str, prev: &mut Option<String>, report: &mut Report) -> Option<()> {
    let mut line = line.trim().to_owned();
    if line.contains("--version is equivalent to version") {
        set(report, "--version is equivalent to version", "PASSED");
    }
    if let Some((test, _)) = line.split_once(" ... ") {
        *prev = Some(test.to_owned());
    }
    if let Some(suffix) = [" ... ok", " ... OK", " ...  OK"]
        .into_iter()
        .find(|s| line.ends_with(s))
    {
        if line.starts_with("Applying sites.0002_alter_domain_unique...test_no_migrations") {
            line = line
                .split_once("...")
                .map_or("", |(_, rest)| rest)
                .trim()
                .to_owned();
        }
        let test = line.rsplit_once(suffix).map_or(line.as_str(), |(t, _)| t);
        set(report, test, "PASSED");
    }
    if let Some((test, _)) = line.split_once(" ... skipped") {
        set(report, test, "SKIPPED");
    }
    if line.ends_with(" ... FAIL") {
        set(
            report,
            line.split(" ... FAIL").next().unwrap_or_default(),
            "FAILED",
        );
    }
    if line.starts_with("FAIL:") {
        set(report, second_word(&line)?, "FAILED");
    }
    if line.ends_with(" ... ERROR") {
        set(
            report,
            line.split(" ... ERROR").next().unwrap_or_default(),
            "ERROR",
        );
    }
    if line.starts_with("ERROR:") {
        set(report, second_word(&line)?, "ERROR");
    }
    if let Some(test) = prev.as_deref().filter(|_| line.starts_with("ok")) {
        set(report, test, "PASSED");
    }
    Some(())
}

pub fn parse_log_django(log: &str) -> Option<Report> {
    let mut report = Report::new();
    let mut prev = None;
    for line in log.split('\n') {
        django_line(line, &mut prev, &mut report)?;
    }
    for pattern in DJANGO_PATTERNS.iter() {
        for c in pattern.captures_iter(log) {
            set(&mut report, &c[1], "PASSED");
        }
    }
    Some(report)
}

static COLOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(\d+)m").unwrap());

pub fn parse_log_pytest_v2(log: &str) -> Option<Report> {
    let mut report = Report::new();
    for line in log.split('\n') {
        let line: String = COLOR
            .replace_all(line, "")
            .chars()
            .filter(|c| !('\u{1}'..='\u{1f}').contains(c))
            .collect();
        if starts_with_status(&line) {
            let (status, test) = status_line(&line)?;
            set(&mut report, &test, &status);
        } else if STATUSES.iter().any(|s| line.ends_with(s)) {
            let mut words = line.split_whitespace();
            let (Some(test), Some(status)) = (words.next(), words.next()) else {
                return None;
            };
            set(&mut report, test, status);
        }
    }
    Some(report)
}

pub fn parse_log_seaborn(log: &str) -> Option<Report> {
    let mut report = Report::new();
    for line in log.split('\n') {
        if line.starts_with("FAILED") {
            set(&mut report, second_word(line)?, "FAILED");
        } else if line.contains(" PASSED ") {
            let mut words = line.split_whitespace();
            let (Some(test), Some(status)) = (words.next(), words.next()) else {
                return None;
            };
            if status == "PASSED" {
                set(&mut report, test, "PASSED");
            }
        } else if line.starts_with("PASSED") {
            set(&mut report, second_word(line)?, "PASSED");
        }
    }
    Some(report)
}

static SYMPY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(_*) (.*)\.py:(.*) (_*)").unwrap());

pub fn parse_log_sympy(log: &str) -> Option<Report> {
    let mut report = Report::new();
    for c in SYMPY.captures_iter(log) {
        set(&mut report, &format!("{}.py:{}", &c[2], &c[3]), "FAILED");
    }
    for line in log.split('\n') {
        let mut line = line.trim();
        if !line.starts_with("test_") {
            continue;
        }
        if line.ends_with("[FAIL]") || line.ends_with("[OK]") {
            line = line[..line.rfind('[').unwrap_or(line.len())].trim();
        }
        for (suffix, status) in [(" E", "ERROR"), (" F", "FAILED"), (" ok", "PASSED")] {
            if line.ends_with(suffix) {
                set(
                    &mut report,
                    line.split_whitespace().next().unwrap_or(line),
                    status,
                );
            }
        }
    }
    Some(report)
}

pub fn parse_log_matplotlib(log: &str) -> Option<Report> {
    let replaced = log
        .replace("MouseButton.LEFT", "1")
        .replace("MouseButton.RIGHT", "3");
    parse_log_pytest(&replaced)
}

pub fn parser_name(repo: &str) -> Option<&'static str> {
    Some(match repo {
        "django/django" => "parse_log_django",
        "matplotlib/matplotlib" => "parse_log_matplotlib",
        "mwaskom/seaborn" => "parse_log_seaborn",
        "sympy/sympy" => "parse_log_sympy",
        "astropy/astropy" | "scikit-learn/scikit-learn" | "sphinx-doc/sphinx" => {
            "parse_log_pytest_v2"
        }
        "psf/requests" | "pydicom/pydicom" | "pylint-dev/pylint" => "parse_log_pytest_options",
        "marshmallow-code/marshmallow"
        | "pallets/flask"
        | "pvlib/pvlib-python"
        | "pydata/xarray"
        | "pylint-dev/astroid"
        | "pytest-dev/pytest"
        | "pyvista/pyvista"
        | "sqlfluff/sqlfluff"
        | "rustyhgm" => "parse_log_pytest",
        _ => return None,
    })
}

pub fn parser(name: &str) -> Option<Parser> {
    Some(match name {
        "parse_log_pytest" => parse_log_pytest,
        "parse_log_pytest_options" => parse_log_pytest_options,
        "parse_log_django" => parse_log_django,
        "parse_log_pytest_v2" => parse_log_pytest_v2,
        "parse_log_seaborn" => parse_log_seaborn,
        "parse_log_sympy" => parse_log_sympy,
        "parse_log_matplotlib" => parse_log_matplotlib,
        _ => return None,
    })
}

pub fn parser_for_repo(repo: &str) -> Option<Parser> {
    parser_name(repo).and_then(parser)
}
