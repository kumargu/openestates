use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::models::{Interest, InterestCount, InterestResponse};
use crate::state::AppState;

/// Max interest submissions per rate-limit window (60 seconds).
const RATE_LIMIT_MAX: u32 = 60;
/// Rate-limit window duration.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

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

    // --- Rate limiting (global, not per-IP) ---
    {
        let mut limiter = state.interest_rate_limiter.write().await;
        let now = Instant::now();
        if now.duration_since(limiter.0) > RATE_LIMIT_WINDOW {
            // Reset window
            *limiter = (now, 1);
        } else if limiter.1 >= RATE_LIMIT_MAX {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(InterestError {
                    error: "rate limit exceeded — try again shortly".to_string(),
                }),
            ));
        } else {
            limiter.1 += 1;
        }
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
        buyer_name: req
            .buyer_name
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(String::from),
        buyer_contact: req
            .buyer_contact
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(String::from),
        created_at: now.to_rfc3339(),
    };

    // Persist to data/interests/{property_id}.jsonl (append-only)
    let interests_dir = state.project_root.join("data").join("interests");
    if let Err(e) = tokio::fs::create_dir_all(&interests_dir).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(InterestError {
                error: format!("failed to create interests directory: {}", e),
            }),
        ));
    }

    let file_path = interests_dir.join(format!("{}.jsonl", property_id));
    let line = match serde_json::to_string(&interest) {
        Ok(j) => format!("{}\n", j),
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(InterestError {
                    error: format!("failed to serialize interest: {}", e),
                }),
            ));
        }
    };

    // Append to JSONL file
    use tokio::io::AsyncWriteExt;
    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(InterestError {
                    error: format!("failed to open interests file: {}", e),
                }),
            ));
        }
    };

    if let Err(e) = file.write_all(line.as_bytes()).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(InterestError {
                error: format!("failed to write interest: {}", e),
            }),
        ));
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

    // NOTE: Reads JSONL on every GET. Acceptable at current scale (single-digit
    // interests per property). Scaling path: maintain an in-memory counter map
    // updated on write, with periodic JSONL reconciliation.
    let count = crate::utils::count_lines(&file_path).await;

    Ok(Json(InterestCount {
        property_id: id,
        count,
    }))
}
