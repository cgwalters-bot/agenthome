use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::{Map, Value};

pub mod eval;
pub mod install;

/// The repository uses whole-line JSON comments, not general JSONC.
pub fn parse_jsonc(text: &str) -> Result<Value> {
    let json = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::from_str(&json).context("parse JSON with whole-line comments")
}

pub fn read_jsonc(path: &Path) -> Result<Value> {
    parse_jsonc(&fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

/// Whether JavaScript treats `key` as an array index, which its objects list
/// before every other key, in ascending order.
fn is_array_index(key: &str) -> bool {
    (key == "0" || !key.starts_with('0'))
        && key.bytes().all(|b| b.is_ascii_digit())
        && key.parse::<u32>().is_ok_and(|index| index < u32::MAX)
}

/// Give a value the shape `JSON.stringify` writes: its number spelling (for
/// example, 2.0 becomes 2), and array-index keys first in ascending order,
/// then the remaining keys in insertion order.
pub fn javascript_order(value: &Value) -> Result<Value> {
    Ok(match value {
        Value::Number(number) => {
            let float = number
                .as_f64()
                .filter(|value| value.is_finite())
                .context("JSON number outside finite f64 range")?;
            let mut buffer = ryu_js::Buffer::new();
            Value::Number(
                buffer
                    .format_finite(float)
                    .parse()
                    .context("format JSON number")?,
            )
        }
        Value::Array(values) => {
            Value::Array(values.iter().map(javascript_order).collect::<Result<_>>()?)
        }
        Value::Object(values) => {
            let (mut indices, names): (Vec<_>, Vec<_>) =
                values.iter().partition(|(key, _)| is_array_index(key));
            indices.sort_by_key(|(key, _)| key.parse::<u32>().expect("array index"));
            let mut ordered = Map::new();
            for (key, value) in indices.into_iter().chain(names) {
                ordered.insert(key.clone(), javascript_order(value)?);
            }
            Value::Object(ordered)
        }
        _ => value.clone(),
    })
}

/// `JSON.stringify(value, null, 2)`, as the configuration files are written.
pub fn pretty_json(value: &Value) -> Result<String> {
    serde_json::to_string_pretty(&javascript_order(value)?).context("serialize configuration")
}
