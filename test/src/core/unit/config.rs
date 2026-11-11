use std::path::{Path, PathBuf};

use hgm_core::config::{ExpansionStatistic, HgmConfig, OverrideValue};
use serde_json::Value;

use crate::common::{case, cases, differs, fixtures_dir};

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

fn shipped_config() -> HgmConfig {
    HgmConfig::from_yaml_str(&shipped_yaml(), "config.yaml")
}

fn every_field_with_its_type() -> Vec<(&'static str, OverrideValue)> {
    vec![
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
    ]
}

fn wrong_values_for(value: &OverrideValue) -> Vec<OverrideValue> {
    [
        OverrideValue::Str("x".into()),
        OverrideValue::Int(-1),
        OverrideValue::Bool(true),
    ]
    .into_iter()
    .filter(|w| std::mem::discriminant(w) != std::mem::discriminant(value))
    .filter(|w| !(matches!(value, OverrideValue::Float(_)) && matches!(w, OverrideValue::Int(_))))
    .collect()
}

fn config_with_every_field_set() -> HgmConfig {
    let mut config = HgmConfig::default();
    for (key, value) in every_field_with_its_type() {
        config.set(key, value);
    }
    config
}

fn loaded_with_overrides() -> HgmConfig {
    let path = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(path.path(), shipped_yaml()).unwrap();
    HgmConfig::load(
        path.path(),
        &[
            ("optimization.alpha", None),
            ("execution.max_workers", Some(OverrideValue::Int(4))),
        ],
    )
}

fn the_shipped_config() -> HgmConfig {
    let shipped =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../config.yaml");
    HgmConfig::from_yaml(&shipped)
}

fn yaml_with_a_float_and_an_empty_string() -> String {
    let mut config = HgmConfig::default();
    config.set("optimization.alpha", OverrideValue::Float(0.1));
    config.set(
        "paths.initial_agent_name",
        OverrideValue::Str(String::new()),
    );
    config.to_yaml()
}

fn set_expansion_statistic(config: &mut HgmConfig, name: &str) -> bool {
    config.set(
        "optimization.expansion_statistic",
        OverrideValue::Str(name.into()),
    )
}

#[test]
fn matches_the_python_config() {
    let mismatches: Vec<String> = cases("config")
        .iter()
        .filter(|case| {
            let name = case["name"].as_str().unwrap();
            name != LOADS_DEFAULTS_INSTEAD && name != IGNORES_A_SECTION_OVERRIDE
        })
        .filter_map(|case| {
            differs(
                &load(&case["input"]).to_json(),
                &case["output"],
                case["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(mismatches, Vec::<String>::new());
}

#[test]
fn a_wrongly_typed_value_loads_the_defaults() {
    let input = &case("config", LOADS_DEFAULTS_INSTEAD)["input"];
    assert_eq!(load(input), HgmConfig::default());
}

#[test]
fn the_python_rejects_a_section_name_override() {
    assert_eq!(
        case("config", IGNORES_A_SECTION_OVERRIDE)["output"]["error"],
        "AttributeError"
    );
}

#[test]
fn a_section_name_override_is_ignored() {
    let input = &case("config", IGNORES_A_SECTION_OVERRIDE)["input"];
    assert_eq!(load(input), shipped_config());
}

#[test]
fn setting_a_whole_section_is_refused() {
    let mut config = HgmConfig::default();
    assert!(!config.set("optimization", OverrideValue::Int(3)));
}

#[test]
fn setting_a_float_field_to_a_string_is_refused() {
    let mut config = HgmConfig::default();
    assert!(!config.set("optimization.alpha", OverrideValue::Str("x".into())));
}

#[test]
fn setting_a_float_field_to_an_int_is_accepted() {
    let mut config = HgmConfig::default();
    assert!(config.set("optimization.alpha", OverrideValue::Int(1)));
}

#[test]
fn an_int_set_on_a_float_field_is_stored_as_that_float() {
    let mut config = HgmConfig::default();
    config.set("optimization.alpha", OverrideValue::Int(1));
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
    assert_eq!(the_shipped_config().paths.output_dir, None);
}

#[test]
fn the_shipped_config_turns_cool_down_off() {
    assert!(!the_shipped_config().optimization.cool_down);
}

#[test]
fn the_shipped_config_starts_from_the_default_agent() {
    assert_eq!(
        the_shipped_config().paths.initial_agent_name,
        "default_agent"
    );
}

#[test]
fn load_skips_overrides_without_a_value() {
    assert_eq!(loaded_with_overrides().optimization.alpha, 0.6);
}

#[test]
fn load_applies_overrides_with_a_value() {
    assert_eq!(loaded_with_overrides().execution.max_workers, 4);
}

#[test]
fn set_takes_each_field_with_its_type() {
    let mut config = HgmConfig::default();
    let refused: Vec<&str> = every_field_with_its_type()
        .into_iter()
        .filter(|(key, value)| !config.set(key, value.clone()))
        .map(|(key, _)| key)
        .collect();
    assert_eq!(refused, Vec::<&str>::new());
}

#[test]
fn an_int_set_on_beta_is_stored_as_a_float() {
    assert_eq!(
        config_with_every_field_set().to_json()["optimization"]["beta"],
        2.0
    );
}

#[test]
fn set_stores_the_max_task_evals() {
    assert_eq!(
        config_with_every_field_set().to_json()["execution"]["max_task_evals"],
        9
    );
}

#[test]
fn set_stores_the_run_to_continue_from() {
    assert_eq!(
        config_with_every_field_set().to_json()["paths"]["continue_from"],
        "prev"
    );
}

#[test]
fn set_refuses_each_field_a_value_of_another_type() {
    let config = config_with_every_field_set();
    let accepted: Vec<&str> = every_field_with_its_type()
        .into_iter()
        .filter(|(key, value)| {
            wrong_values_for(value)
                .into_iter()
                .any(|w| config.clone().set(key, w))
        })
        .map(|(key, _)| key)
        .collect();
    assert_eq!(accepted, Vec::<&str>::new());
}

#[test]
fn set_refuses_a_key_without_a_section() {
    let mut config = config_with_every_field_set();
    assert!(!config.set("nodot", OverrideValue::Bool(true)));
}

#[test]
fn set_refuses_an_unknown_field() {
    let mut config = config_with_every_field_set();
    assert!(!config.set("llm.unknown", OverrideValue::Bool(true)));
}

#[test]
fn refused_sets_leave_the_config_unchanged() {
    let mut config = config_with_every_field_set();
    let json = config.to_json();
    for (key, value) in every_field_with_its_type() {
        for wrong in wrong_values_for(&value) {
            config.set(key, wrong);
        }
    }
    config.set("nodot", OverrideValue::Bool(true));
    config.set("llm.unknown", OverrideValue::Bool(true));
    assert_eq!(config.to_json(), json);
}

#[test]
fn yaml_writes_floats_like_python() {
    let yaml = yaml_with_a_float_and_an_empty_string();
    assert!(yaml.contains("  alpha: 0.1\n"), "{yaml}");
}

#[test]
fn yaml_writes_empty_strings_quoted() {
    let yaml = yaml_with_a_float_and_an_empty_string();
    assert!(yaml.contains("  initial_agent_name: ''\n"), "{yaml}");
}

#[test]
fn yaml_writes_an_unset_path_as_null() {
    let yaml = yaml_with_a_float_and_an_empty_string();
    assert!(yaml.contains("  output_dir: null\n"), "{yaml}");
}

#[test]
fn the_expansion_statistic_is_read_by_name() {
    let own = HgmConfig::parse_yaml("optimization:\n  expansion_statistic: own\n").unwrap();
    assert_eq!(
        own.optimization.expansion_statistic,
        ExpansionStatistic::Own
    );
}

#[test]
fn the_expansion_statistic_defaults_to_the_clade() {
    assert_eq!(
        HgmConfig::default().optimization.expansion_statistic,
        ExpansionStatistic::Clade
    );
}

#[test]
fn an_unknown_expansion_statistic_in_yaml_is_an_error() {
    assert!(HgmConfig::parse_yaml("optimization:\n  expansion_statistic: both\n").is_err());
}

#[test]
fn setting_an_unknown_expansion_statistic_is_refused() {
    let mut config = HgmConfig::default();
    assert!(!set_expansion_statistic(&mut config, "both"));
}

#[test]
fn setting_the_own_expansion_statistic_is_accepted() {
    let mut config = HgmConfig::default();
    assert!(set_expansion_statistic(&mut config, "own"));
}

#[test]
fn setting_the_clade_expansion_statistic_is_accepted() {
    let mut config = HgmConfig::default();
    set_expansion_statistic(&mut config, "own");
    assert!(set_expansion_statistic(&mut config, "clade"));
}

#[test]
fn the_last_expansion_statistic_set_wins() {
    let mut config = HgmConfig::default();
    set_expansion_statistic(&mut config, "both");
    set_expansion_statistic(&mut config, "own");
    set_expansion_statistic(&mut config, "clade");
    assert_eq!(
        config.optimization.expansion_statistic,
        ExpansionStatistic::Clade
    );
}

#[test]
fn the_expansion_statistic_parses_own() {
    assert_eq!(
        ExpansionStatistic::parse("own"),
        Some(ExpansionStatistic::Own)
    );
}
