use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_skills))
        .route("/installable", get(list_installable))
        .route("/learned", get(list_learned))
        .route("/learned", post(create_learned))
        .route("/learned/{name}", get(get_learned))
        .route("/learned/{name}", put(patch_learned))
        .route("/learned/{name}", delete(deactivate_learned))
        .route("/{name}/view", get(view_skill))
        .route("/installable", post(create_installable))
        .route("/installable/{name}", delete(delete_installable))
        .route("/reload", post(reload_installable))
}

/// List all skills (both installable and learned) — names + descriptions.
async fn list_skills(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let store = state.skill_store.read().await;
    let summaries = store.all_skill_summaries().await;

    let skills: Vec<serde_json::Value> = summaries
        .into_iter()
        .map(|(name, desc, source)| {
            serde_json::json!({
                "name": name,
                "description": desc,
                "source": source,
            })
        })
        .collect();

    Json(serde_json::json!({ "skills": skills }))
}

/// List installable skills only.
async fn list_installable(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let store = state.skill_store.read().await;
    let skills: Vec<serde_json::Value> = store
        .list_installable()
        .into_iter()
        .map(|(name, desc)| {
            serde_json::json!({
                "name": name,
                "description": desc,
                "source": "installed",
            })
        })
        .collect();

    Json(serde_json::json!({ "skills": skills }))
}

/// List learned skills only (with full metadata).
async fn list_learned(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let store = state.skill_store.read().await;
    match store.list_learned().await {
        Ok(skills) => {
            let items: Vec<serde_json::Value> = skills
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "name": s.name,
                        "description": s.description,
                        "category": s.category,
                        "author": s.author,
                        "usage_count": s.usage_count,
                        "success_rate": s.success_rate,
                        "version": s.version,
                        "is_active": s.is_active,
                        "created_at": s.created_at.to_rfc3339(),
                        "last_used_at": s.last_used_at.map(|t| t.to_rfc3339()),
                        "last_improved_at": s.last_improved_at.map(|t| t.to_rfc3339()),
                    })
                })
                .collect();
            Json(serde_json::json!({ "skills": items }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// View full skill content by name (installable or learned).
async fn view_skill(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.skill_store.read().await;

    // Check installable first
    if let Some(skill) = store.get_installable(&name) {
        return Ok(Json(serde_json::json!({
            "name": skill.frontmatter.name,
            "description": skill.frontmatter.description,
            "version": skill.frontmatter.version,
            "author": skill.frontmatter.author,
            "source": "installed",
            "content": skill.body,
            "source_path": skill.source_path,
        })));
    }

    // Check learned
    if let Ok(Some(skill)) = store.get_learned_by_name(&name).await {
        return Ok(Json(serde_json::json!({
            "name": skill.name,
            "description": skill.description,
            "version": skill.version,
            "author": skill.author,
            "source": "learned",
            "content": skill.content,
            "category": skill.category,
            "usage_count": skill.usage_count,
            "success_rate": skill.success_rate,
            "created_at": skill.created_at.to_rfc3339(),
            "last_used_at": skill.last_used_at.map(|t| t.to_rfc3339()),
            "last_improved_at": skill.last_improved_at.map(|t| t.to_rfc3339()),
        })));
    }

    Err(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct CreateLearnedRequest {
    name: String,
    description: Option<String>,
    content: String,
    category: Option<String>,
}

/// Create a new learned skill.
async fn create_learned(
    State(state): State<AppState>,
    Json(req): Json<CreateLearnedRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let store = state.skill_store.write().await;

    // Check for conflicts
    if store.get_installable(&req.name).is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "An installable skill with this name already exists" })),
        ));
    }
    if let Ok(Some(_)) = store.get_learned_by_name(&req.name).await {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "A learned skill with this name already exists" })),
        ));
    }

    match store
        .create_learned(
            &req.name,
            req.description.as_deref(),
            &req.content,
            req.category.as_deref(),
            "admin",
            None,
        )
        .await
    {
        Ok(skill) => Ok(Json(serde_json::json!({
            "id": skill.id,
            "name": skill.name,
            "version": skill.version,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// Get a learned skill by name.
async fn get_learned(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let store = state.skill_store.read().await;
    match store.get_learned_by_name(&name).await {
        Ok(Some(skill)) => Ok(Json(serde_json::json!({
            "id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "content": skill.content,
            "category": skill.category,
            "author": skill.author,
            "usage_count": skill.usage_count,
            "success_rate": skill.success_rate,
            "version": skill.version,
            "is_active": skill.is_active,
            "created_at": skill.created_at.to_rfc3339(),
            "last_used_at": skill.last_used_at.map(|t| t.to_rfc3339()),
            "last_improved_at": skill.last_improved_at.map(|t| t.to_rfc3339()),
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

#[derive(Deserialize)]
struct PatchLearnedRequest {
    content: String,
    description: Option<String>,
}

/// Patch (update) a learned skill's content.
async fn patch_learned(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<PatchLearnedRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let store = state.skill_store.write().await;

    let existing = match store.get_learned_by_name(&name).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Skill not found" })),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
    };

    match store
        .patch_learned(existing.id, &req.content, req.description.as_deref())
        .await
    {
        Ok(()) => Ok(Json(serde_json::json!({
            "name": name,
            "version": existing.version + 1,
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// Deactivate a learned skill.
async fn deactivate_learned(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let store = state.skill_store.write().await;

    let existing = match store.get_learned_by_name(&name).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Skill not found" })),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
    };

    match store.deactivate_learned(existing.id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "deactivated": name }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

#[derive(Deserialize)]
struct CreateInstallableRequest {
    name: String,
    description: String,
    content: String,
    version: Option<String>,
    author: Option<String>,
}

/// Create a new installable skill (writes SKILL.md to disk).
async fn create_installable(
    State(state): State<AppState>,
    Json(req): Json<CreateInstallableRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut store = state.skill_store.write().await;

    if store.get_installable(&req.name).is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "An installable skill with this name already exists" })),
        ));
    }

    let frontmatter = velkor_skills::parser::SkillFrontmatter {
        name: req.name.clone(),
        description: req.description,
        version: req.version,
        author: req.author,
        license: None,
        compatibility: None,
        platforms: vec![],
        metadata: serde_json::Value::Null,
    };

    let skill = velkor_skills::parser::SkillDefinition {
        frontmatter,
        body: req.content,
        source_path: None,
    };

    match store.save_installable(&skill) {
        Ok(path) => Ok(Json(serde_json::json!({
            "name": req.name,
            "path": path.display().to_string(),
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// Delete an installable skill from disk.
async fn delete_installable(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let mut store = state.skill_store.write().await;

    match store.delete_installable(&name) {
        Ok(()) => Ok(Json(serde_json::json!({ "deleted": name }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )),
    }
}

/// Reload installable skills from disk (rescan directories).
async fn reload_installable(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let mut store = state.skill_store.write().await;
    let count = store.load_installable_skills();
    Json(serde_json::json!({ "reloaded": count }))
}
