use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_user))
        .route("/by-email/{email}", get(get_by_email))
}

#[derive(Deserialize)]
struct CreateUserRequest {
    email: String,
    password_hash: String,
    name: Option<String>,
}

async fn create_user(
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), axum::http::StatusCode> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        INSERT INTO users (email, password_hash, display_name, role)
        VALUES ($1, $2, $3, 'member')
        RETURNING id, email, role
        "#,
    )
    .bind(&req.email)
    .bind(&req.password_hash)
    .bind(&req.name)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
            axum::http::StatusCode::CONFLICT
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::json!({
            "id": row.id,
            "email": row.email,
            "role": row.role,
        })),
    ))
}

async fn get_by_email(
    State(state): State<AppState>,
    Path(email): Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query_as::<_, UserWithHashRow>(
        r#"
        SELECT id, email, password_hash, role, org_id
        FROM users
        WHERE email = $1 AND is_active = TRUE
        "#,
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "id": row.id,
        "email": row.email,
        "password_hash": row.password_hash,
        "role": row.role,
        "org_id": row.org_id,
    })))
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    role: String,
}

#[derive(sqlx::FromRow)]
struct UserWithHashRow {
    id: Uuid,
    email: String,
    password_hash: String,
    role: String,
    org_id: Option<Uuid>,
}
