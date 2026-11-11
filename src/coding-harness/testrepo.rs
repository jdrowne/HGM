pub const SWE_TEST_PROMPT: &str = "The tests in the repository can be run with the bash command `{test_command}`. If no specific test files are provided, all tests will be run. The given command-line options must be used EXACTLY as specified. Do not use any other command-line options. {test_hint}";

pub const HGM_TEST_DESCRIPTION: &str = "The tests in the repository can be run with the bash command `cd /rustyhgm/ && cargo test -p rustyhgm-coding-harness --test unit <test name filter>`. If no test name filter is provided, all tests will be run. The given command-line options must be used EXACTLY as specified. Do not use any other command-line options. ONLY test tools and utils. NEVER try to test or run the harness's forward().";

pub const DOTTED_TEST_HINT: &str = "If the target test file path is tests/some_folder/some_file.py, then <specific test files> should be `some_folder.some_file`.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRepo {
    Hgm,
    Swe,
    Polyglot,
}

pub fn get_test_command(eval_script: &str) -> (String, String) {
    let script = eval_script.trim();
    let line = script.rsplit('\n').nth(1).unwrap_or(script);
    let mut parts: Vec<&str> = line.split_whitespace().collect();
    let mut hint = String::new();
    if let Some(last) = parts.last()
        && last.contains('.')
        && !last.ends_with(".py")
    {
        hint = DOTTED_TEST_HINT.to_owned();
    }
    while parts.last().is_some_and(|p| p.contains('.')) {
        parts.pop();
    }
    (
        format!("cd /testbed/ && {} <specific test files>", parts.join(" ")),
        hint,
    )
}

pub fn get_test_description(eval_script: &str, repo: TestRepo) -> String {
    let description = match repo {
        TestRepo::Swe => {
            let (command, hint) = get_test_command(eval_script);
            SWE_TEST_PROMPT
                .replace("{test_hint}", &hint)
                .replace("{test_command}", &command)
        }
        TestRepo::Polyglot => format!(
            "In the repository folder, the tests can be run with the following bash command(s):\n\n```{eval_script}```\n"
        ),
        TestRepo::Hgm => HGM_TEST_DESCRIPTION.to_owned(),
    };
    description.trim().to_owned()
}
