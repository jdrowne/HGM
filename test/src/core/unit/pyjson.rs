use hgm_core::pyjson::{PyValue, dumps, f64_lenient, float_repr, parse_lenient};

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
    for (value, expected) in cases {
        assert_eq!(float_repr(value), expected, "{value:e}");
    }
}

#[test]
fn dumps_matches_python_separators_and_escapes() {
    let value = PyValue::dict([
        (
            "a",
            PyValue::List(vec![PyValue::Int(1), PyValue::Float(2.0), PyValue::Null]),
        ),
        ("é", PyValue::str("tab\t\"q\" 😀")),
        ("empty", PyValue::List(vec![])),
        ("none", PyValue::dict(Vec::<(String, PyValue)>::new())),
        ("b", PyValue::Bool(true)),
    ]);
    assert_eq!(
        dumps(&value, None),
        r#"{"a": [1, 2.0, null], "\u00e9": "tab\t\"q\" \ud83d\ude00", "empty": [], "none": {}, "b": true}"#
    );
    assert_eq!(
        dumps(&value, Some(2)),
        "{\n  \"a\": [\n    1,\n    2.0,\n    null\n  ],\n  \"\\u00e9\": \"tab\\t\\\"q\\\" \\ud83d\\ude00\",\n  \"empty\": [],\n  \"none\": {},\n  \"b\": true\n}"
    );
}

#[test]
fn bare_non_finite_tokens_are_read_back() {
    let v = parse_lenient(
        r#"{"m": Infinity, "n": -Infinity, "o": NaN, "s": "Infinity NaN", "x": 1.5}"#,
    )
    .unwrap();
    assert_eq!(f64_lenient(&v["m"]), Some(f64::INFINITY));
    assert_eq!(f64_lenient(&v["n"]), Some(f64::NEG_INFINITY));
    assert!(f64_lenient(&v["o"]).unwrap().is_nan());
    assert_eq!(v["s"], "Infinity NaN");
    assert_eq!(f64_lenient(&v["x"]), Some(1.5));
    assert_eq!(f64_lenient(&v["s"]), None);
}
