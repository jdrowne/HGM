use rustyhgm_core::pyjson::{PyValue, dumps, f64_lenient, float_repr, parse_lenient};
use serde_json::{Value, json};

#[test]
fn floats_are_written_like_python_repr() {
    let cases = [
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (1.0, "1.0"),
        (0.5, "0.5"),
        (0.1, "0.1"),
        (0.6666666666666666, "0.6666666666666666"),
        (0.0001, "0.0001"),
        (0.00001, "1e-05"),
        (1.5e-7, "1.5e-07"),
        (123456789.0, "123456789.0"),
        (1e15, "1000000000000000.0"),
        (1e16, "1e+16"),
        (1.2345e20, "1.2345e+20"),
        (-2.5, "-2.5"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
        (f64::NAN, "NaN"),
    ];
    let actual: Vec<String> = cases.iter().map(|(v, _)| float_repr(*v)).collect();
    let expected: Vec<&str> = cases.iter().map(|(_, e)| *e).collect();
    assert_eq!(actual, expected);
}

fn mixed() -> PyValue {
    PyValue::dict([
        (
            "a",
            PyValue::List(vec![PyValue::Int(1), PyValue::Float(2.0), PyValue::Null]),
        ),
        ("é", PyValue::str("tab\t\"q\" 😀")),
        ("empty", PyValue::List(vec![])),
        ("none", PyValue::dict(Vec::<(String, PyValue)>::new())),
        ("b", PyValue::Bool(true)),
    ])
}

#[test]
fn dumps_matches_python_separators_and_escapes() {
    assert_eq!(
        dumps(&mixed(), None),
        r#"{"a": [1, 2.0, null], "\u00e9": "tab\t\"q\" \ud83d\ude00", "empty": [], "none": {}, "b": true}"#
    );
}

#[test]
fn indented_dumps_matches_python() {
    assert_eq!(
        dumps(&mixed(), Some(2)),
        "{\n  \"a\": [\n    1,\n    2.0,\n    null\n  ],\n  \"\\u00e9\": \"tab\\t\\\"q\\\" \\ud83d\\ude00\",\n  \"empty\": [],\n  \"none\": {},\n  \"b\": true\n}"
    );
}

fn bare() -> Value {
    parse_lenient(r#"{"m": Infinity, "n": -Infinity, "o": NaN, "s": "Infinity NaN", "x": 1.5}"#)
        .unwrap()
}

#[test]
fn a_bare_infinity_is_read_back() {
    assert_eq!(f64_lenient(&bare()["m"]), Some(f64::INFINITY));
}

#[test]
fn a_bare_negative_infinity_is_read_back() {
    assert_eq!(f64_lenient(&bare()["n"]), Some(f64::NEG_INFINITY));
}

#[test]
fn a_bare_nan_is_read_back() {
    assert!(f64_lenient(&bare()["o"]).unwrap().is_nan());
}

#[test]
fn non_finite_words_inside_strings_are_left_alone() {
    assert_eq!(bare()["s"], "Infinity NaN");
}

#[test]
fn finite_numbers_next_to_bare_tokens_are_read() {
    assert_eq!(f64_lenient(&bare()["x"]), Some(1.5));
}

#[test]
fn a_string_is_not_a_number() {
    assert_eq!(f64_lenient(&bare()["s"]), None);
}

#[test]
fn every_json_value_converts_and_dumps_like_python() {
    let v = json!({"n": null, "t": true, "f": false, "i": -3, "x": 1.5, "s": "q\"\\\n\r\t\u{8}\u{c}é😀", "l": [], "d": {}, "nl": [1, {"k": []}]});
    assert_eq!(
        dumps(&PyValue::from(&v), None),
        r#"{"n": null, "t": true, "f": false, "i": -3, "x": 1.5, "s": "q\"\\\n\r\t\b\f\u00e9\ud83d\ude00", "l": [], "d": {}, "nl": [1, {"k": []}]}"#
    );
}

#[test]
fn indented_empty_and_string_lists_dump_like_python() {
    assert_eq!(
        dumps(
            &PyValue::dict([
                ("l", PyValue::List(vec![])),
                ("d", PyValue::Dict(vec![])),
                ("v", PyValue::list_of_str(&["a"]))
            ]),
            Some(2)
        ),
        "{\n  \"l\": [],\n  \"d\": {},\n  \"v\": [\n    \"a\"\n  ]\n}"
    );
}

#[test]
fn an_integer_too_big_for_i64_becomes_a_float() {
    assert_eq!(
        PyValue::from(&json!(u64::MAX)),
        PyValue::Float(u64::MAX as f64)
    );
}

fn quoted() -> Value {
    parse_lenient(r#"{"a": NaN, "b": -Infinity, "c": Infinity, "s": "say \"NaN\" \\", "d": 2}"#)
        .unwrap()
}

#[test]
fn lenient_parsing_reads_nan() {
    assert!(f64_lenient(&quoted()["a"]).unwrap().is_nan());
}

#[test]
fn lenient_parsing_reads_negative_infinity() {
    assert_eq!(f64_lenient(&quoted()["b"]), Some(f64::NEG_INFINITY));
}

#[test]
fn lenient_parsing_reads_infinity() {
    assert_eq!(f64_lenient(&quoted()["c"]), Some(f64::INFINITY));
}

#[test]
fn lenient_parsing_keeps_escaped_quotes_in_strings() {
    assert_eq!(quoted()["s"], "say \"NaN\" \\");
}

#[test]
fn lenient_parsing_reads_integers_as_floats() {
    assert_eq!(f64_lenient(&quoted()["d"]), Some(2.0));
}

#[test]
fn a_string_with_escapes_is_not_a_number() {
    assert_eq!(f64_lenient(&quoted()["s"]), None);
}

#[test]
fn null_is_not_a_number() {
    assert_eq!(f64_lenient(&json!(null)), None);
}
