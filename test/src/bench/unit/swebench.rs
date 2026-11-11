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

fn missing() -> Swebench {
    Swebench {
        python: "hgm-no-such-python".to_owned(),
    }
}

#[test]
fn specs_without_an_interpreter_are_an_error() {
    assert!(missing().specs(&[serde_json::json!({})]).is_err());
}

#[test]
fn preparing_images_without_an_interpreter_is_an_error() {
    assert!(missing().prepare_images("d", &["x"], 1).is_err());
}

#[test]
fn preparing_no_images_needs_no_interpreter() {
    let none: [&str; 0] = [];
    assert!(missing().prepare_images("d", &none, 1).is_ok());
}

#[test]
fn errors_say_what_failed() {
    assert_eq!(
        hgm_bench::swebench::Error::from(std::io::Error::other("x")).to_string(),
        "running swebench: x"
    );
}

#[test]
fn the_environment_always_names_an_interpreter() {
    assert!(!Swebench::from_env().python.is_empty());
}
