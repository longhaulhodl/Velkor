//! SKILL.md parser — extracts YAML frontmatter and markdown body.

use serde::{Deserialize, Serialize};

/// Parsed skill from a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
    /// Where this skill was loaded from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// YAML frontmatter for a skill, following the agentskills.io spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub compatibility: Option<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Validation error for a SKILL.md file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("missing YAML frontmatter (file must start with ---)")]
    MissingFrontmatter,
    #[error("malformed frontmatter: {0}")]
    MalformedFrontmatter(String),
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("invalid skill name: {0}")]
    InvalidName(String),
    #[error("empty body — skill must have instructions after frontmatter")]
    EmptyBody,
    #[error("content too large ({0} bytes, max {1})")]
    TooLarge(usize, usize),
}

const MAX_SKILL_SIZE: usize = 100_000;
const MAX_NAME_LEN: usize = 64;
const MAX_DESC_LEN: usize = 1024;

/// Parse a SKILL.md file into frontmatter + body.
pub fn parse_skill_md(content: &str) -> Result<SkillDefinition, ParseError> {
    if content.len() > MAX_SKILL_SIZE {
        return Err(ParseError::TooLarge(content.len(), MAX_SKILL_SIZE));
    }

    let content = content.trim();
    if !content.starts_with("---") {
        return Err(ParseError::MissingFrontmatter);
    }

    // Find closing ---
    let after_first = &content[3..];
    let end_idx = after_first
        .find("\n---")
        .ok_or(ParseError::MissingFrontmatter)?;

    let yaml_str = &after_first[..end_idx].trim();
    let body_start = 3 + end_idx + 4; // skip past \n---
    let body = content[body_start..].trim().to_string();

    if body.is_empty() {
        return Err(ParseError::EmptyBody);
    }

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_str)
        .map_err(|e| ParseError::MalformedFrontmatter(e.to_string()))?;

    // Validate required fields
    if frontmatter.name.is_empty() {
        return Err(ParseError::MissingField("name".into()));
    }
    if frontmatter.description.is_empty() {
        return Err(ParseError::MissingField("description".into()));
    }

    // Validate name format
    validate_name(&frontmatter.name)?;

    if frontmatter.description.len() > MAX_DESC_LEN {
        return Err(ParseError::InvalidName(format!(
            "description too long ({} chars, max {})",
            frontmatter.description.len(),
            MAX_DESC_LEN
        )));
    }

    Ok(SkillDefinition {
        frontmatter,
        body,
        source_path: None,
    })
}

/// Validate skill name: lowercase, a-z0-9 and hyphens, no leading/trailing hyphens.
fn validate_name(name: &str) -> Result<(), ParseError> {
    if name.len() > MAX_NAME_LEN {
        return Err(ParseError::InvalidName(format!(
            "too long ({} chars, max {})",
            name.len(),
            MAX_NAME_LEN
        )));
    }

    if name.starts_with('-') || name.ends_with('-') {
        return Err(ParseError::InvalidName(
            "cannot start or end with hyphen".into(),
        ));
    }

    if name.contains("--") {
        return Err(ParseError::InvalidName(
            "cannot contain consecutive hyphens".into(),
        ));
    }

    for ch in name.chars() {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-' && ch != '.' && ch != '_'
        {
            return Err(ParseError::InvalidName(format!(
                "invalid character '{ch}', only a-z, 0-9, hyphens, dots, underscores allowed"
            )));
        }
    }

    Ok(())
}

/// Generate a SKILL.md string from frontmatter + body.
pub fn render_skill_md(frontmatter: &SkillFrontmatter, body: &str) -> String {
    let yaml = serde_yaml::to_string(frontmatter).unwrap_or_default();
    format!("---\n{yaml}---\n\n{body}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_skill() {
        let content = r#"---
name: test-skill
description: A test skill for unit testing.
version: "1.0"
---

# Test Skill

Do the thing.
"#;
        let skill = parse_skill_md(content).unwrap();
        assert_eq!(skill.frontmatter.name, "test-skill");
        assert_eq!(skill.frontmatter.version.as_deref(), Some("1.0"));
        assert!(skill.body.contains("Do the thing."));
    }

    #[test]
    fn reject_missing_frontmatter() {
        let result = parse_skill_md("# Just a markdown file");
        assert!(matches!(result, Err(ParseError::MissingFrontmatter)));
    }

    #[test]
    fn reject_empty_body() {
        let content = "---\nname: empty\ndescription: Nothing here\n---\n";
        let result = parse_skill_md(content);
        assert!(matches!(result, Err(ParseError::EmptyBody)));
    }

    #[test]
    fn reject_invalid_name() {
        let content = "---\nname: UPPERCASE\ndescription: bad\n---\n\nBody.";
        let result = parse_skill_md(content);
        assert!(matches!(result, Err(ParseError::InvalidName(_))));
    }

    #[test]
    fn roundtrip_render() {
        let fm = SkillFrontmatter {
            name: "my-skill".into(),
            description: "Does stuff.".into(),
            version: Some("2.0".into()),
            author: None,
            license: None,
            compatibility: None,
            platforms: vec![],
            metadata: serde_json::Value::Null,
        };
        let rendered = render_skill_md(&fm, "# Instructions\n\nDo the thing.");
        let reparsed = parse_skill_md(&rendered).unwrap();
        assert_eq!(reparsed.frontmatter.name, "my-skill");
        assert!(reparsed.body.contains("Do the thing."));
    }
}
