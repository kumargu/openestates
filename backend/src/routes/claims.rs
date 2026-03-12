use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct ClaimRequest {
    pub property_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Claim {
    pub property_id: String,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub claimed_at: String,
}

#[derive(Serialize)]
pub struct ClaimResponse {
    pub status: &'static str,
    pub property_id: String,
}

#[derive(Serialize)]
pub struct ClaimError {
    pub error: String,
}

/// POST /api/claims — submit a property ownership claim.
pub async fn submit_claim(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClaimRequest>,
) -> Result<(StatusCode, Json<ClaimResponse>), (StatusCode, Json<ClaimError>)> {
    // Validate name is non-empty
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ClaimError {
                error: "name is required".to_string(),
            }),
        ));
    }

    // Validate at least one contact method
    let phone = req.phone.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from);
    let email = req.email.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from);
    if phone.is_none() && email.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ClaimError {
                error: "at least one of phone or email is required".to_string(),
            }),
        ));
    }

    // Validate property exists
    let property_id = req.property_id.trim().to_string();
    {
        let properties = state.properties.read().await;
        if !properties.iter().any(|p| p.id == property_id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ClaimError {
                    error: format!("property '{}' not found", property_id),
                }),
            ));
        }
    }

    // Build the claim
    let claim = Claim {
        property_id: property_id.clone(),
        name,
        phone,
        email,
        claimed_at: chrono::Utc::now().to_rfc3339(),
    };

    // Persist to data/claims/{property_id}.json
    let claims_dir = state.project_root.join("data").join("claims");
    if let Err(e) = tokio::fs::create_dir_all(&claims_dir).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ClaimError {
                error: format!("failed to create claims directory: {}", e),
            }),
        ));
    }

    let file_path = claims_dir.join(format!("{}.json", property_id));
    let mut claims = read_existing_claims(&file_path).await;
    claims.push(claim);

    let json = match serde_json::to_string_pretty(&claims) {
        Ok(j) => j,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ClaimError {
                    error: format!("failed to serialize claims: {}", e),
                }),
            ));
        }
    };

    if let Err(e) = tokio::fs::write(&file_path, json).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ClaimError {
                error: format!("failed to write claims file: {}", e),
            }),
        ));
    }

    Ok((
        StatusCode::CREATED,
        Json(ClaimResponse {
            status: "claim_received",
            property_id,
        }),
    ))
}

async fn read_existing_claims(path: &Path) -> Vec<Claim> {
    match tokio::fs::read_to_string(path).await {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}
