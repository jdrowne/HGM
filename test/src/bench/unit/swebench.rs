use hgm_bench::swebench::{Swebench, TestSpec};

#[test]
fn container_names_follow_swebench() {
    let spec = TestSpec {
        instance_id: "django__django-11099".to_owned(),
        eval_script: String::new(),
        image: String::new(),
        platform: "linux/x86_64".to_owned(),
        user: "root".to_owned(),
        nano_cpus: None,
    };
    assert_eq!(
        spec.container_name("r1"),
        "sweb.eval.django__django-11099.r1"
    );
}

#[test]
fn a_missing_interpreter_is_an_error() {
    let swebench = Swebench {
        python: "hgm-no-such-python".to_owned(),
    };
    assert!(swebench.specs(&[serde_json::json!({})]).is_err());
    assert!(swebench.prepare_images("d", &["x"], 1).is_err());
    let none: [&str; 0] = [];
    assert!(swebench.prepare_images("d", &none, 1).is_ok());
}
