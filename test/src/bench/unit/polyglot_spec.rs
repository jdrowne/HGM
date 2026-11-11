use hgm_bench::polyglot_spec::{
    LANGUAGES, arch_for, get_eval_script, host_arch, make_test_spec, test_cmd, test_commands,
};
use serde_json::json;

use crate::common::cases;

#[test]
fn specs_match_the_python() {
    for case in cases("polyglot_specs") {
        let name = case["name"].as_str().unwrap();
        let want = &case["output"];
        if let Some(language) = case["input"]["language"].as_str() {
            assert_eq!(
                get_eval_script(test_commands(language)),
                want["eval_script"],
                "{name}"
            );
            assert_eq!(test_cmd(language), want["test_cmd"], "{name}");
            continue;
        }
        let input = &case["input"];
        let spec = make_test_spec(&input["instance"], input["machine"].as_str().unwrap());
        let got = json!({
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
        });
        for (key, value) in want.as_object().unwrap() {
            assert_eq!(&got[key], value, "{name} {key}");
        }
    }
}

#[test]
fn every_language_has_test_commands() {
    for language in LANGUAGES {
        assert!(!test_commands(language).is_empty(), "{language}");
    }
    assert!(test_commands("cobol").is_empty());
    assert_eq!(test_cmd("cobol"), "");
}

#[test]
fn machines_map_to_the_two_architectures() {
    assert_eq!(arch_for("aarch64"), "arm64");
    assert_eq!(arch_for("arm64"), "arm64");
    assert_eq!(arch_for("x86_64"), "x86_64");
    assert_eq!(arch_for("riscv64"), "x86_64");
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
fn only_well_formed_new_file_headers_count_as_added() {
    assert_eq!(reset_line(""), "true");
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
    for (patch, want) in cases {
        assert_eq!(reset_line(patch), want, "{patch:?}");
    }
}

#[test]
fn container_names_without_a_run_id_have_no_suffix() {
    let spec = make_test_spec(&json!({"instance_id": "rust__a"}), "arm64");
    assert_eq!(spec.container_name(""), "pb.eval.rust__a");
    assert_eq!(spec.platform(), "linux/arm64/v8");
}
