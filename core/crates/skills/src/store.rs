//! Skill storage — filesystem for installable skills, DB for learned skills.

use crate::parser::{self, SkillDefinition};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// A learned skill stored in the database.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LearnedSkill {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub category: Option<String>,
    pub author: String,
    pub source_conversation_id: Option<Uuid>,
    pub usage_count: i32,
    pub success_rate: f32,
    pub version: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_improved_at: Option<DateTime<Utc>>,
}

/// Unified skill store — manages both installable (file) and learned (DB) skills.
pub struct SkillStore {
    /// Directories to scan for installable SKILL.md files.
    skill_dirs: Vec<PathBuf>,
    /// Cached installable skills (loaded on startup and refresh).
    installable: HashMap<String, SkillDefinition>,
    /// Database pool for learned skills.
    pool: PgPool,
}

impl SkillStore {
    pub fn new(pool: PgPool, skill_dirs: Vec<PathBuf>) -> Self {
        Self {
            skill_dirs,
            installable: HashMap::new(),
            pool,
        }
    }

    // -----------------------------------------------------------------------
    // Installable skills (filesystem)
    // -----------------------------------------------------------------------

    /// Scan all skill directories and load SKILL.md files.
    pub fn load_installable_skills(&mut self) -> usize {
        self.installable.clear();
        let mut count = 0;

        let dirs: Vec<PathBuf> = self.skill_dirs.clone();
        for dir in &dirs {
            if !dir.exists() {
                debug!(path = %dir.display(), "Skills directory does not exist, skipping");
                continue;
            }
            count += self.scan_directory(dir);
        }

        info!(count, dirs = self.skill_dirs.len(), "Loaded installable skills");
        count
    }

    fn scan_directory(&mut self, dir: &Path) -> usize {
        let mut count = 0;
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(path = %dir.display(), error = %e, "Failed to read skills directory");
                return 0;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Check for SKILL.md inside the directory
                let skill_path = path.join("SKILL.md");
                if skill_path.exists() {
                    if let Some(skill) = self.load_skill_file(&skill_path) {
                        self.installable.insert(skill.frontmatter.name.clone(), skill);
                        count += 1;
                    }
                }
                // Also recurse one level for category directories
                count += self.scan_directory(&path);
            } else if path.file_name().map_or(false, |f| f == "SKILL.md") {
                // SKILL.md directly in the skills dir (no subdirectory)
                if let Some(skill) = self.load_skill_file(&path) {
                    self.installable.insert(skill.frontmatter.name.clone(), skill);
                    count += 1;
                }
            }
        }

        count
    }

    fn load_skill_file(&self, path: &Path) -> Option<SkillDefinition> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to read skill file");
                return None;
            }
        };

        match parser::parse_skill_md(&content) {
            Ok(mut skill) => {
                skill.source_path = Some(path.to_string_lossy().into_owned());
                debug!(name = %skill.frontmatter.name, path = %path.display(), "Loaded skill");
                Some(skill)
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "Failed to parse skill");
                None
            }
        }
    }

    /// Get an installable skill by name.
    pub fn get_installable(&self, name: &str) -> Option<&SkillDefinition> {
        self.installable.get(name)
    }

    /// List all installable skill names + descriptions (for the system prompt index).
    pub fn list_installable(&self) -> Vec<(&str, &str)> {
        let mut skills: Vec<_> = self
            .installable
            .values()
            .map(|s| (s.frontmatter.name.as_str(), s.frontmatter.description.as_str()))
            .collect();
        skills.sort_by_key(|(name, _)| *name);
        skills
    }

    /// Save a new installable skill to disk.
    pub fn save_installable(&mut self, skill: &SkillDefinition) -> Result<PathBuf, anyhow::Error> {
        let base_dir = self.skill_dirs.first().ok_or_else(|| {
            anyhow::anyhow!("no skills directory configured")
        })?;

        let skill_dir = base_dir.join(&skill.frontmatter.name);
        std::fs::create_dir_all(&skill_dir)?;

        let skill_path = skill_dir.join("SKILL.md");
        let content = parser::render_skill_md(&skill.frontmatter, &skill.body);
        std::fs::write(&skill_path, &content)?;

        // Update cache
        let mut cached = skill.clone();
        cached.source_path = Some(skill_path.to_string_lossy().into_owned());
        self.installable.insert(skill.frontmatter.name.clone(), cached);

        info!(name = %skill.frontmatter.name, path = %skill_path.display(), "Saved installable skill");
        Ok(skill_path)
    }

    /// Delete an installable skill from disk and cache.
    pub fn delete_installable(&mut self, name: &str) -> Result<(), anyhow::Error> {
        let base_dir = self.skill_dirs.first().ok_or_else(|| {
            anyhow::anyhow!("no skills directory configured")
        })?;

        let skill_dir = base_dir.join(name);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)?;
        }
        self.installable.remove(name);
        info!(name, "Deleted installable skill");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Learned skills (database)
    // -----------------------------------------------------------------------

    /// Create a new learned skill in the database.
    pub async fn create_learned(
        &self,
        name: &str,
        description: Option<&str>,
        content: &str,
        category: Option<&str>,
        author: &str,
        conversation_id: Option<Uuid>,
    ) -> Result<LearnedSkill, sqlx::Error> {
        let row = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
            r#"
            INSERT INTO skills (name, description, content, category, author, source_conversation_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, created_at
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(content)
        .bind(category)
        .bind(author)
        .bind(conversation_id)
        .fetch_one(&self.pool)
        .await?;

        info!(name, id = %row.0, "Created learned skill");

        Ok(LearnedSkill {
            id: row.0,
            name: name.to_string(),
            description: description.map(String::from),
            content: content.to_string(),
            category: category.map(String::from),
            author: author.to_string(),
            source_conversation_id: conversation_id,
            usage_count: 0,
            success_rate: 1.0,
            version: 1,
            is_active: true,
            created_at: row.1,
            last_used_at: None,
            last_improved_at: None,
        })
    }

    /// Patch a learned skill's content (increments version).
    pub async fn patch_learned(
        &self,
        id: Uuid,
        new_content: &str,
        new_description: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE skills
            SET content = $2,
                description = COALESCE($3, description),
                version = version + 1,
                last_improved_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(new_content)
        .bind(new_description)
        .execute(&self.pool)
        .await?;

        info!(id = %id, "Patched learned skill");
        Ok(())
    }

    /// Record that a learned skill was used (updates usage_count and last_used_at).
    pub async fn record_usage(&self, id: Uuid, success: bool) -> Result<(), sqlx::Error> {
        // Update usage count and recalculate success_rate with exponential moving average
        let weight = if success { 1.0_f32 } else { 0.0_f32 };
        sqlx::query(
            r#"
            UPDATE skills
            SET usage_count = usage_count + 1,
                last_used_at = now(),
                success_rate = success_rate * 0.9 + $2 * 0.1
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(weight)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List all active learned skills (name + description for index).
    pub async fn list_learned(&self) -> Result<Vec<LearnedSkill>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, String, Option<String>, String, Option<Uuid>, i32, f32, i32, bool, DateTime<Utc>, Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
            r#"
            SELECT id, name, description, content, category, author,
                   source_conversation_id, usage_count, success_rate, version,
                   is_active, created_at, last_used_at, last_improved_at
            FROM skills
            WHERE is_active = TRUE
            ORDER BY usage_count DESC, name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| LearnedSkill {
                id: r.0,
                name: r.1,
                description: r.2,
                content: r.3,
                category: r.4,
                author: r.5,
                source_conversation_id: r.6,
                usage_count: r.7,
                success_rate: r.8,
                version: r.9,
                is_active: r.10,
                created_at: r.11,
                last_used_at: r.12,
                last_improved_at: r.13,
            })
            .collect())
    }

    /// Get a learned skill by name.
    pub async fn get_learned_by_name(&self, name: &str) -> Result<Option<LearnedSkill>, sqlx::Error> {
        let row = sqlx::query_as::<_, (Uuid, String, Option<String>, String, Option<String>, String, Option<Uuid>, i32, f32, i32, bool, DateTime<Utc>, Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
            r#"
            SELECT id, name, description, content, category, author,
                   source_conversation_id, usage_count, success_rate, version,
                   is_active, created_at, last_used_at, last_improved_at
            FROM skills
            WHERE name = $1 AND is_active = TRUE
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| LearnedSkill {
            id: r.0,
            name: r.1,
            description: r.2,
            content: r.3,
            category: r.4,
            author: r.5,
            source_conversation_id: r.6,
            usage_count: r.7,
            success_rate: r.8,
            version: r.9,
            is_active: r.10,
            created_at: r.11,
            last_used_at: r.12,
            last_improved_at: r.13,
        }))
    }

    /// Deactivate a learned skill.
    pub async fn deactivate_learned(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE skills SET is_active = FALSE WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        info!(id = %id, "Deactivated learned skill");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Unified listing (both types)
    // -----------------------------------------------------------------------

    /// Get all skill names + descriptions (both installable and learned) for the
    /// system prompt skills index.
    pub async fn all_skill_summaries(&self) -> Vec<(String, String, &'static str)> {
        let mut summaries: Vec<(String, String, &'static str)> = Vec::new();

        // Installable skills
        for (name, desc) in self.list_installable() {
            summaries.push((name.to_string(), desc.to_string(), "installed"));
        }

        // Learned skills
        if let Ok(learned) = self.list_learned().await {
            for skill in learned {
                let desc = skill.description.unwrap_or_else(|| {
                    skill.content.chars().take(100).collect::<String>()
                });
                summaries.push((skill.name, desc, "learned"));
            }
        }

        summaries.sort_by(|a, b| a.0.cmp(&b.0));
        summaries
    }

    /// Get full skill content by name (checks installable first, then learned).
    pub async fn get_skill_content(&self, name: &str) -> Option<String> {
        // Check installable first
        if let Some(skill) = self.installable.get(name) {
            return Some(skill.body.clone());
        }

        // Check learned
        if let Ok(Some(skill)) = self.get_learned_by_name(name).await {
            return Some(skill.content);
        }

        None
    }
}
