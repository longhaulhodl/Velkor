use crate::{Tool, ToolContext, ToolError, ToolResult};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value as JsonValue};
use tracing::debug;

/// Fetches a URL and returns cleaned text content.
///
/// Strips HTML tags to produce readable text. Truncates to
/// `max_content_length` to avoid overwhelming the model context.
pub struct WebFetchTool {
    client: Client,
    max_content_length: usize,
}

impl WebFetchTool {
    pub fn new(max_content_length: usize) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            max_content_length,
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its text content. HTML is stripped to produce readable text. Use this to read web pages, documentation, or API responses."
    }

    fn input_schema(&self) -> JsonValue {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, input: JsonValue, _ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let url = input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'url' field".into()))?;

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ToolError::InvalidInput(
                "URL must start with http:// or https://".into(),
            ));
        }

        debug!(url, "Fetching URL");

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "Velkor/0.1 (web_fetch tool)")
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("fetch failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(ToolError::ExecutionFailed(format!(
                "HTTP {} from {url}",
                resp.status()
            )));
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = resp
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to read body: {e}")))?;

        // Clean the content based on type
        let cleaned = if content_type.contains("text/html") {
            strip_html(&body)
        } else {
            body
        };

        // Truncate if needed
        let truncated = if cleaned.len() > self.max_content_length {
            format!(
                "{}\n\n[Content truncated at {} characters]",
                &cleaned[..self.max_content_length],
                self.max_content_length
            )
        } else {
            cleaned
        };

        Ok(ToolResult::success(truncated))
    }
}

/// Simple HTML tag stripper. Removes tags, decodes common entities,
/// and collapses whitespace. Not a full parser — good enough for
/// extracting readable text from web pages.
fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut last_was_space = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if !in_tag && i + 7 < len && &lower[i..i + 7] == "<script" {
            in_script = true;
            in_tag = true;
            i += 1;
            continue;
        }
        if in_script && i + 9 <= len && &lower[i..i + 9] == "</script>" {
            in_script = false;
            in_tag = false;
            i += 9;
            continue;
        }
        if !in_tag && i + 6 < len && &lower[i..i + 6] == "<style" {
            in_style = true;
            in_tag = true;
            i += 1;
            continue;
        }
        if in_style && i + 8 <= len && &lower[i..i + 8] == "</style>" {
            in_style = false;
            in_tag = false;
            i += 8;
            continue;
        }

        if in_script || in_style {
            i += 1;
            continue;
        }

        let ch = chars[i];

        if ch == '<' {
            in_tag = true;
            // Block-level tags get a newline
            if i + 3 < len {
                let next3: String = lower_chars[i + 1..len.min(i + 4)].iter().collect();
                if next3.starts_with("br")
                    || next3.starts_with("p")
                    || next3.starts_with("/p")
                    || next3.starts_with("div")
                    || next3.starts_with("/di")
                    || next3.starts_with("h1")
                    || next3.starts_with("h2")
                    || next3.starts_with("h3")
                    || next3.starts_with("li")
                {
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    last_was_space = true;
                }
            }
            i += 1;
            continue;
        }

        if ch == '>' {
            in_tag = false;
            i += 1;
            continue;
        }

        if in_tag {
            i += 1;
            continue;
        }

        // Decode HTML entities
        if ch == '&' && i + 1 < len {
            let rest = &html[i..len.min(i + 10)];
            if let Some(end) = rest.find(';') {
                let entity = &rest[..end + 1];
                let decoded = match entity {
                    "&amp;" => "&",
                    "&lt;" => "<",
                    "&gt;" => ">",
                    "&quot;" => "\"",
                    "&apos;" | "&#39;" => "'",
                    "&nbsp;" => " ",
                    _ => "",
                };
                if !decoded.is_empty() {
                    result.push_str(decoded);
                    last_was_space = decoded == " ";
                    i += entity.len();
                    continue;
                }
            }
        }

        // Collapse whitespace
        if ch.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(ch);
            last_was_space = false;
        }

        i += 1;
    }

    // Clean up excessive newlines
    let mut cleaned = String::new();
    let mut newline_count = 0;
    for ch in result.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                cleaned.push(ch);
            }
        } else {
            newline_count = 0;
            cleaned.push(ch);
        }
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        let html = "<h1>Hello</h1><p>World</p>";
        let text = strip_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }

    #[test]
    fn test_strip_html_entities() {
        let html = "a &amp; b &lt; c &gt; d";
        let text = strip_html(html);
        assert_eq!(text, "a & b < c > d");
    }

    #[test]
    fn test_strip_html_script_removed() {
        let html = "before<script>alert('xss')</script>after";
        let text = strip_html(html);
        assert_eq!(text, "beforeafter");
    }
}
