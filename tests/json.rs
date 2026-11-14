//! Shared JSON serialization semantics used by eval's digest, not config lint.

use agenthome::pretty_json;
use serde_json::Value;

#[test]
fn numbers_use_javascript_spelling() {
    for (input, expected) in [
        ("2.0", "2"),
        ("-0.0", "0"),
        ("0.1", "0.1"),
        ("1e-7", "1e-7"),
        ("1e-6", "0.000001"),
        ("1e20", "100000000000000000000"),
        ("1e21", "1e+21"),
    ] {
        let value = serde_json::from_str(input).unwrap();
        assert_eq!(pretty_json(&value).unwrap(), expected);
    }
}

#[test]
fn json_keys_follow_javascript_order() {
    let value: Value =
        serde_json::from_str(r#"{"b": 1, "10": 2, "a": 3, "9": 4, "01": 5}"#).unwrap();
    let keys: Vec<_> = agenthome::javascript_order(&value)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["9", "10", "b", "a", "01"]);
}
