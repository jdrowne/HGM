use rustyhgm_bench::polyglot_spec::{
    LANGUAGES, TestSpec, arch_for, get_eval_script, host_arch, make_test_spec, test_cmd,
    test_commands,
};
use serde_json::json;

use crate::common::cases;

fn spec_values(case: &serde_json::Value) -> serde_json::Value {
    let input = &case["input"];
    let spec = make_test_spec(&input["instance"], input["machine"].as_str().unwrap());
    json!({
        "arch": spec.arch,
        "platform": spec.platform(),
        "base_image_key": spec.base_image_key(),
        "env_image_key": spec.env_image_key(),
        "instance_image_key": spec.instance_image_key(),
        "container_name": spec.container_name("r1"),
        "setup_env_script": spec.setup_env_script(),
        "eval_script": spec.eval_script(),
        "install_repo_script": spec.install_repo_script(),
        "base_dockerfile": spec.base_dockerfile(),
        "env_dockerfile": spec.env_dockerfile(),
        "instance_dockerfile": spec.instance_dockerfile(),
    })
}

fn spec_mismatches(
    languages: bool,
    got: impl Fn(&serde_json::Value) -> serde_json::Value,
) -> Vec<String> {
    cases("polyglot_specs")
        .iter()
        .filter(|case| case["input"]["language"].is_string() == languages)
        .flat_map(|case| {
            let actual = got(case);
            case["output"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(key, value)| &actual[key.as_str()] != *value)
                .map(|(key, _)| format!("{} {key}", case["name"]))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn language_scripts_match_the_python() {
    let found = spec_mismatches(true, |case| {
        let language = case["input"]["language"].as_str().unwrap();
        json!({
            "eval_script": get_eval_script(test_commands(language)),
            "test_cmd": test_cmd(language),
        })
    });
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn specs_match_the_python() {
    assert_eq!(spec_mismatches(false, spec_values), Vec::<String>::new());
}

#[test]
fn every_language_has_test_commands() {
    let missing: Vec<&str> = LANGUAGES
        .iter()
        .copied()
        .filter(|language| test_commands(language).is_empty())
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
}

#[test]
fn an_unknown_language_has_no_test_commands() {
    assert!(test_commands("cobol").is_empty());
}

#[test]
fn an_unknown_language_has_no_test_command() {
    assert_eq!(test_cmd("cobol"), "");
}

#[test]
fn aarch64_is_arm64() {
    assert_eq!(arch_for("aarch64"), "arm64");
}

#[test]
fn arm64_is_arm64() {
    assert_eq!(arch_for("arm64"), "arm64");
}

#[test]
fn x86_64_is_x86_64() {
    assert_eq!(arch_for("x86_64"), "x86_64");
}

#[test]
fn other_machines_are_x86_64() {
    assert_eq!(arch_for("riscv64"), "x86_64");
}

#[test]
fn the_host_architecture_follows_the_build_target() {
    assert_eq!(host_arch(), arch_for(std::env::consts::ARCH));
}

fn reset_line(test_patch: &str) -> String {
    let instance = json!({
        "instance_id": "go__t",
        "language": "go",
        "repo": "r",
        "base_commit": "c",
        "test_patch": test_patch,
    });
    make_test_spec(&instance, "x86_64").eval_script_list[10].clone()
}

#[test]
fn an_empty_test_patch_resets_nothing() {
    assert_eq!(reset_line(""), "true");
}

#[test]
fn only_well_formed_new_file_headers_count_as_added() {
    let cases = [
        ("diff --git a/x b/x\nnew file mode 100644\n", "rm -f x"),
        ("diff --git a/x b/y\nnew file mode 100644\n", "true"),
        ("diff --git a/ b/\nnew file mode 100644\n", "true"),
        ("diff --git a/x y b/x y\nnew file mode 100644\n", "true"),
        ("diff --git a/xyz\nnew file mode 100644\n", "true"),
        ("diff --git b/x a/x\nnew file mode 100644\n", "true"),
        ("diff --git a/x b/x\nindex 1..2\n", "true"),
        ("x --- a/m.go\n--- a/n.go\n", "git checkout c -- m.go n.go"),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter(|(patch, want)| reset_line(patch) != *want)
        .map(|(patch, _)| format!("{patch:?}"))
        .collect();
    assert_eq!(wrong, Vec::<String>::new());
}

fn arm_spec() -> TestSpec {
    make_test_spec(&json!({"instance_id": "rust__a"}), "arm64")
}

#[test]
fn container_names_without_a_run_id_have_no_suffix() {
    assert_eq!(arm_spec().container_name(""), "pb.eval.rust__a");
}

#[test]
fn arm64_specs_use_the_v8_platform() {
    assert_eq!(arm_spec().platform(), "linux/arm64/v8");
}

fn renamed() -> TestSpec {
    TestSpec {
        namespace: "fake".to_owned(),
        ..arm_spec()
    }
}

#[test]
fn a_namespace_renames_the_base_image() {
    assert_eq!(renamed().base_image_key(), "fake.base.arm64:latest");
}

#[test]
fn a_namespace_renames_the_env_image() {
    assert!(renamed().env_image_key().starts_with("fake.env.arm64."));
}

#[test]
fn a_namespace_renames_the_instance_image() {
    assert_eq!(
        renamed().instance_image_key(),
        "fake.eval.arm64.rust__a:latest"
    );
}

#[test]
fn a_namespace_renames_the_container() {
    assert_eq!(renamed().container_name(""), "fake.eval.rust__a");
}

#[test]
fn a_namespace_renames_the_container_of_a_run() {
    assert_eq!(renamed().container_name("r"), "fake.eval.rust__a.r");
}

#[test]
fn a_namespace_renames_the_env_dockerfile_base() {
    assert!(
        renamed()
            .env_dockerfile()
            .starts_with("FROM --platform=linux/arm64/v8 fake.base.arm64:latest\n")
    );
}

#[test]
fn a_namespace_renames_the_instance_dockerfile_base() {
    assert!(
        renamed()
            .instance_dockerfile()
            .starts_with("FROM --platform=linux/arm64/v8 fake.env.arm64.")
    );
}

#[test]
fn the_instance_dockerfile_copies_the_folder_the_repo_starts_in() {
    let spec = TestSpec {
        repo: "polyglot-benchmark/javascript/exercises/practice/queen-attack".to_owned(),
        ..arm_spec()
    };
    assert!(
        spec.instance_dockerfile()
            .contains("\nCOPY ./polyglot-benchmark /polyglot-benchmark\n")
    );
}

#[test]
fn a_non_python_env_runs_the_setup_script_file_as_it_is() {
    assert_eq!(
        make_test_spec(&json!({"language": "javascript"}), "x86_64").setup_env_script(),
        include_str!("../../../../src/bench/polyglot_images/setup_env.sh")
    );
}

#[test]
fn the_base_dockerfile_is_the_file_on_every_arch() {
    assert_eq!(
        make_test_spec(&json!({}), "arm64").base_dockerfile(),
        include_str!("../../../../src/bench/polyglot_images/base.Dockerfile")
    );
}
