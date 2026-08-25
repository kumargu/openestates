use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::models::{Interest, InterestCount, InterestResponse};
use crate::security::interest_storage::{
    interest_append_fits, interest_storage_bytes, validate_interest_fields,
};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct InterestRequest {
    pub property_id: String,
    #[serde(default)]
    pub buyer_name: Option<String>,
    #[serde(default)]
    pub buyer_contact: Option<String>,
}

#[derive(Serialize)]
pub struct InterestError {
    pub error: String,
}

/// POST /api/interests — express interest in a property.
pub async fn express_interest(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InterestRequest>,
) -> Result<(StatusCode, Json<InterestResponse>), (StatusCode, Json<InterestError>)> {
    let property_id = req.property_id.trim().to_string();
    if property_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(InterestError {
                error: "property_id is required".to_string(),
            }),
        ));
    }

    let buyer_name = req
        .buyer_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let buyer_contact = req
        .buyer_contact
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Err(error) = validate_interest_fields(buyer_name, buyer_contact) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(InterestError {
                error: error.to_string(),
            }),
        ));
    }

    // Validate property exists
    {
        let properties = state.properties.read().await;
        if !properties.iter().any(|p| p.id == property_id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(InterestError {
                    error: format!("property '{}' not found", property_id),
                }),
            ));
        }
    }

    // Generate interest ID: {property_id}-{timestamp_millis}-{counter}
    let now = chrono::Utc::now();
    let timestamp_millis = now.timestamp_millis();
    let counter = state.interest_counter.fetch_add(1, Ordering::Relaxed);
    let interest_id = format!("{}-{}-{}", property_id, timestamp_millis, counter);

    let interest = Interest {
        id: interest_id.clone(),
        property_id: property_id.clone(),
        buyer_name: buyer_name.map(String::from),
        buyer_contact: buyer_contact.map(String::from),
        created_at: now.to_rfc3339(),
    };

    let line = match serde_json::to_string(&interest) {
        Ok(j) => format!("{}\n", j),
        Err(_) => return Err(interest_storage_unavailable()),
    };

    // Serialize capacity accounting with the append so concurrent requests
    // cannot race past the per-file or deployment-wide storage ceilings.
    let _write_guard = state.interest_write_lock.lock().await;
    let interests_dir = state.project_root.join("data").join("interests");
    if tokio::fs::create_dir_all(&interests_dir).await.is_err() {
        return Err(interest_storage_unavailable());
    }
    let file_path = interests_dir.join(format!("{}.jsonl", property_id));
    let file_bytes = match tokio::fs::metadata(&file_path).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(_) => return Err(interest_storage_unavailable()),
    };
    let storage_bytes = match interest_storage_bytes(&interests_dir).await {
        Ok(bytes) => bytes,
        Err(_) => return Err(interest_storage_unavailable()),
    };
    if !interest_append_fits(file_bytes, storage_bytes, line.len()) {
        return Err(interest_storage_unavailable());
    }

    use tokio::io::AsyncWriteExt;
    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .await
    {
        Ok(f) => f,
        Err(_) => return Err(interest_storage_unavailable()),
    };

    if file.write_all(line.as_bytes()).await.is_err() {
        return Err(interest_storage_unavailable());
    }

    Ok((
        StatusCode::CREATED,
        Json(InterestResponse {
            id: interest_id,
            status: "interest_recorded",
            property_id,
        }),
    ))
}

fn interest_storage_unavailable() -> (StatusCode, Json<InterestError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(InterestError {
            error: "interest storage is temporarily unavailable".to_string(),
        }),
    )
}

/// GET /api/properties/{id}/interests/count — get interest count for a property.
pub async fn get_interest_count(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<InterestCount>, (StatusCode, Json<InterestError>)> {
    // Validate property exists
    {
        let properties = state.properties.read().await;
        if !properties.iter().any(|p| p.id == id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(InterestError {
                    error: format!("property '{}' not found", id),
                }),
            ));
        }
    }

    let file_path = state
        .project_root
        .join("data")
        .join("interests")
        .join(format!("{}.jsonl", id));

    // Reads a file whose size is bounded by the interest storage policy.
    let count = crate::utils::count_lines(&file_path).await;

    Ok(Json(InterestCount {
        property_id: id,
        count,
    }))
}
