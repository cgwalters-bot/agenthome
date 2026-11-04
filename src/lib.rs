use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde_json::Value;

pub mod render;
pub mod tiers;

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

/// Match JSON.stringify's number spelling (for example, 2.0 becomes 2).
/// Preserve object insertion order and the existing two-space indentation.
pub fn pretty_json(value: &Value) -> Result<String> {
    fn normalize(value: &mut Value) -> Result<()> {
        match value {
            Value::Number(number) => {
                let float = number
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .context("JSON number outside finite f64 range")?;
                let mut buffer = ryu_js::Buffer::new();
                *number = buffer
                    .format_finite(float)
                    .parse()
                    .context("format JSON number")?;
            }
            Value::Array(values) => {
                for value in values {
                    normalize(value)?;
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    normalize(value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut value = value.clone();
    normalize(&mut value)?;
    serde_json::to_string_pretty(&value).context("serialize configuration")
}
