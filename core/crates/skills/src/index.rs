//! Skills index — builds the skills section for the system prompt.
//!
//! Follows the progressive disclosure pattern:
//! - Tier 1: Names + descriptions in system prompt (always present)
//! - Tier 2: Full content loaded on demand via skill_view tool
//! - Tier 3: Supporting files loaded via skill_view with file path

use crate::store::SkillStore;

/// Build the skills index block for the system prompt.
///
/// Returns None if no skills are available.
pub async fn build_skills_prompt(store: &SkillStore) -> Option<String> {
    let summaries = store.all_skill_summaries().await;
    if summaries.is_empty() {
        return None;
    }

    let mut lines = Vec::with_capacity(summaries.len() + 10);

    lines.push("## Skills".to_string());
    lines.push(String::new());
    lines.push(
        "Before responding, scan the skills below. If a skill matches or is partially \
         relevant to the task, load it with skill_view(name) and follow its instructions. \
         It is better to load a skill you might not need than to miss critical steps."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("<available_skills>".to_string());

    for (name, desc, source) in &summaries {
        let tag = if *source == "learned" { " [learned]" } else { "" };
        lines.push(format!("  - {name}: {desc}{tag}"));
    }

    lines.push("</available_skills>".to_string());
    lines.push(String::new());
    lines.push(
        "Use skill_view(name) to load a skill's full instructions before following them. \
         If a loaded skill has issues or is outdated, fix it with skill_manage(action='patch'). \
         After completing a complex task (5+ tool calls), consider saving reusable approaches \
         with skill_manage(action='create')."
            .to_string(),
    );

    Some(lines.join("\n"))
}

/// Build the skill review prompt for background post-turn review.
///
/// This is sent to a background agent with the conversation history.
pub fn build_review_prompt() -> &'static str {
    "Review the conversation above and consider saving or updating a skill if appropriate.\n\n\
     Focus on:\n\
     - Was a non-trivial approach used that required trial and error?\n\
     - Did the user expect a different method or outcome, leading to course correction?\n\
     - Was a workflow discovered that would be reusable in future tasks?\n\n\
     If a relevant skill already exists, update it with skill_manage(action='patch').\n\
     Otherwise, create a new skill with skill_manage(action='create') if the approach is reusable.\n\
     If nothing is worth saving, respond with 'Nothing to save.' and stop."
}
