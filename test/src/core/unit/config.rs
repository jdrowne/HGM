use std::path::{Path, PathBuf};

use hgm_core::config::{ExpansionStatistic, HgmConfig, OverrideValue};
use serde_json::Value;

use crate::common::{assert_same, case, cases, fixtures_dir};

const LOADS_DEFAULTS_INSTEAD: &str = "string_where_int_expected";

const IGNORES_A_SECTION_OVERRIDE: &str = "section_name_replaces_whole_section";

fn shipped_yaml() -> String {
    case("config", "shipped_config_yaml")["input"]["yaml"]
        .as_str()
        .unwrap()
        .to_string()
}

fn override_value(v: &Value) -> Option<OverrideValue> {
    match v {
        Value::Null => None,
        Value::Bool(b) => Some(OverrideValue::Bool(*b)),
        Value::String(s) => Some(OverrideValue::Str(s.clone())),
        Value::Number(n) => Some(match n.as_i64() {
            Some(i) => OverrideValue::Int(i),
            None => OverrideValue::Float(n.as_f64().unwrap()),
        }),
        other => panic!("no override value for {other}"),
    }
}

fn load(input: &Value) -> HgmConfig {
    let mut config = match &input["yaml"] {
        Value::Null => HgmConfig::from_yaml(Path::new("/nonexistent/hgm/config.yaml")),
        Value::String(s) if s == "<shipped config.yaml>" => {
            HgmConfig::from_yaml_str(&shipped_yaml(), "config.yaml")
        }
        Value::String(s) => HgmConfig::from_yaml_str(s, "c.yaml"),
        other => panic!("no yaml in {other}"),
    };
    if let Some(overrides) = input["overrides"].as_object() {
        for (key, value) in overrides {
            if let Some(v) = override_value(value) {
                config.set(key, v);
            }
        }
    }
    config
}

#[test]
fn matches_the_python_config() {
    for case in cases("config") {
        let name = case["name"].as_str().unwrap();
        if name == LOADS_DEFAULTS_INSTEAD || name == IGNORES_A_SECTION_OVERRIDE {
            continue;
        }
        assert_same(&load(&case["input"]).to_json(), &case["output"], name);
    }
}

#[test]
fn a_wrongly_typed_value_loads_the_defaults() {
    let input = &case("config", LOADS_DEFAULTS_INSTEAD)["input"];
    assert_eq!(load(input), HgmConfig::default());
}

#[test]
fn a_section_name_override_is_ignored() {
    let input = &case("config", IGNORES_A_SECTION_OVERRIDE)["input"];
    assert_eq!(
        case("config", IGNORES_A_SECTION_OVERRIDE)["output"]["error"],
        "AttributeError"
    );
    assert_eq!(
        load(input),
        HgmConfig::from_yaml_str(&shipped_yaml(), "config.yaml")
    );
    let mut config = HgmConfig::default();
    assert!(!config.set("optimization", OverrideValue::Int(3)));
    assert!(!config.set("optimization.alpha", OverrideValue::Str("x".into())));
    assert!(config.set("optimization.alpha", OverrideValue::Int(1)));
    assert_eq!(config.optimization.alpha, 1.0);
}

#[test]
fn saved_defaults_match_the_python_yaml() {
    let expected =
        std::fs::read_to_string(fixtures_dir().join("config/defaults_saved.yaml")).unwrap();
    assert_eq!(HgmConfig::default().to_yaml(), expected);
}

#[test]
fn the_shipped_config_leaves_the_output_dir_unset() {
    let shipped =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../config.yaml");
    let config = HgmConfig::from_yaml(&shipped);
    assert_eq!(config.paths.output_dir, None);
    assert!(!config.optimization.cool_down);
    assert_eq!(config.paths.initial_agent_name, "default_agent");
}

#[test]
fn load_skips_overrides_without_a_value() {
    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), shipped_yaml()).unwrap();
    let config = HgmConfig::load(
        path.path(),
        &[
            ("optimization.alpha", None),
            ("execution.max_workers", Some(OverrideValue::Int(4))),
        ],
    );
    assert_eq!(config.optimization.alpha, 0.6);
    assert_eq!(config.execution.max_workers, 4);
}

#[test]
fn set_takes_each_field_only_with_its_type() {
    let fields = [
        ("llm.self_improve_llm", OverrideValue::Str("m".into())),
        ("llm.downstream_llm", OverrideValue::Str("m".into())),
        ("llm.diagnose_llm", OverrideValue::Str("m".into())),
        ("optimization.alpha", OverrideValue::Float(0.5)),
        ("optimization.beta", OverrideValue::Int(2)),
        ("optimization.cool_down", OverrideValue::Bool(false)),
        ("optimization.eval_random_level", OverrideValue::Float(0.25)),
        (
            "optimization.n_pseudo_descendant_evals",
            OverrideValue::Int(7),
        ),
        (
            "optimization.expansion_statistic",
            OverrideValue::Str("own".into()),
        ),
        ("execution.max_workers", OverrideValue::Int(3)),
        ("execution.self_improve_timeout", OverrideValue::Int(60)),
        ("execution.evaluation_timeout", OverrideValue::Int(61)),
        ("execution.max_task_evals", OverrideValue::Int(9)),
        ("evaluation.full_eval", OverrideValue::Bool(true)),
        ("evaluation.polyglot", OverrideValue::Bool(true)),
        ("paths.output_dir", OverrideValue::Str("out".into())),
        ("paths.continue_from", OverrideValue::Str("prev".into())),
        (
            "paths.initial_agent_name",
            OverrideValue::Str("agent".into()),
        ),
    ];
    let mut config = HgmConfig::default();
    for (key, value) in &fields {
        assert!(config.set(key, value.clone()), "{key}");
    }
    let json = config.to_json();
    assert_eq!(json["optimization"]["beta"], 2.0);
    assert_eq!(json["execution"]["max_task_evals"], 9);
    assert_eq!(json["paths"]["continue_from"], "prev");
    let wrong = [
        OverrideValue::Str("x".into()),
        OverrideValue::Int(-1),
        OverrideValue::Bool(true),
    ];
    for (key, value) in &fields {
        let rejected = wrong
            .iter()
            .filter(|w| std::mem::discriminant(*w) != std::mem::discriminant(value))
            .filter(|w| {
                !(matches!(value, OverrideValue::Float(_)) && matches!(w, OverrideValue::Int(_)))
            })
            .all(|w| !config.clone().set(key, w.clone()));
        assert!(rejected, "{key} took a value of the wrong type");
    }
    assert!(!config.set("nodot", OverrideValue::Bool(true)));
    assert!(!config.set("llm.unknown", OverrideValue::Bool(true)));
    assert_eq!(config.to_json(), json);
}

#[test]
fn yaml_writes_floats_like_python_and_empty_strings_quoted() {
    let mut config = HgmConfig::default();
    config.set("optimization.alpha", OverrideValue::Float(0.1));
    config.set(
        "paths.initial_agent_name",
        OverrideValue::Str(String::new()),
    );
    let yaml = config.to_yaml();
    assert!(yaml.contains("  alpha: 0.1\n"), "{yaml}");
    assert!(yaml.contains("  initial_agent_name: ''\n"), "{yaml}");
    assert!(yaml.contains("  output_dir: null\n"), "{yaml}");
}

#[test]
fn the_expansion_statistic_is_read_by_name() {
    let own = HgmConfig::parse_yaml("optimization:\n  expansion_statistic: own\n").unwrap();
    assert_eq!(
        own.optimization.expansion_statistic,
        ExpansionStatistic::Own
    );
    assert_eq!(
        HgmConfig::default().optimization.expansion_statistic,
        ExpansionStatistic::Clade
    );
    assert!(HgmConfig::parse_yaml("optimization:\n  expansion_statistic: both\n").is_err());
    let mut config = HgmConfig::default();
    assert!(!config.set(
        "optimization.expansion_statistic",
        OverrideValue::Str("both".into())
    ));
    assert!(config.set(
        "optimization.expansion_statistic",
        OverrideValue::Str("own".into())
    ));
    assert!(config.set(
        "optimization.expansion_statistic",
        OverrideValue::Str("clade".into())
    ));
    assert_eq!(
        config.optimization.expansion_statistic,
        ExpansionStatistic::Clade
    );
    assert_eq!(
        ExpansionStatistic::parse("own"),
        Some(ExpansionStatistic::Own)
    );
}
