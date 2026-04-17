//! Prompt template rendering for trigger events.
//!
//! Supports `{{path.to.value}}` substitution over a JSON payload.
//! Nested objects are walked via `.`; missing keys render as empty string.
//!
//! Example:
//!   template = "New file at {{file_path}} (sha={{hash}})"
//!   payload  = { "file_path": "/tmp/x.txt", "hash": "abc123" }
//!   result   = "New file at /tmp/x.txt (sha=abc123)"

use regex::Regex;
use std::sync::OnceLock;

static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();

fn re() -> &'static Regex {
    PLACEHOLDER_RE.get_or_init(|| Regex::new(r"\{\{\s*([a-zA-Z0-9_.\[\]]+)\s*\}\}").unwrap())
}

/// Render `template` by substituting `{{key.path}}` against `payload`.
pub fn render(template: &str, payload: &serde_json::Value) -> String {
    re()
        .replace_all(template, |caps: &regex::Captures| {
            let path = &caps[1];
            lookup(payload, path)
                .map(value_to_string)
                .unwrap_or_default()
        })
        .to_string()
}

fn lookup<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in path.split('.') {
        current = match current {
            serde_json::Value::Object(map) => map.get(part)?,
            serde_json::Value::Array(arr) => {
                let idx: usize = part.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }
    Some(current)
}

fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_substitution() {
        let p = json!({ "file_path": "/tmp/x.txt" });
        assert_eq!(render("new: {{file_path}}", &p), "new: /tmp/x.txt");
    }

    #[test]
    fn nested_substitution() {
        let p = json!({ "repo": { "name": "velkor" } });
        assert_eq!(render("repo={{repo.name}}", &p), "repo=velkor");
    }

    #[test]
    fn missing_key_renders_empty() {
        let p = json!({});
        assert_eq!(render("x={{missing}}", &p), "x=");
    }

    #[test]
    fn number_is_stringified() {
        let p = json!({ "n": 42 });
        assert_eq!(render("{{n}}", &p), "42");
    }
}
