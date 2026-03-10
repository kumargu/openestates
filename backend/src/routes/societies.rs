use std::path::PathBuf;

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct SearchQuery {
    #[allow(dead_code)]
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Resolve the path to the ranked results JSON file.
fn ranked_results_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend must be inside project root")
        .join("data")
        .join("intelligence")
        .join("whitefield")
        .join("_ranked_results.json")
}

/// GET /api/societies/search?q=... — returns pre-computed ranked society results.
pub async fn search_societies(
    Query(_params): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let path = ranked_results_path();

    let contents = std::fs::read_to_string(&path).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Intelligence data not yet available. Run the enrichment pipeline first."
                    .to_string(),
            }),
        )
    })?;

    let json: serde_json::Value = serde_json::from_str(&contents).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to parse ranked results JSON.".to_string(),
            }),
        )
    })?;

    Ok(Json(json))
}

/// GET /api/societies/:slug — returns a single society detail from the ranked results.
pub async fn get_society(
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let path = ranked_results_path();

    let contents = std::fs::read_to_string(&path).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Intelligence data not yet available. Run the enrichment pipeline first."
                    .to_string(),
            }),
        )
    })?;

    let json: serde_json::Value = serde_json::from_str(&contents).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to parse ranked results JSON.".to_string(),
            }),
        )
    })?;

    // The ranked results should be an array; find the entry matching the slug by society_id.
    let results = json
        .as_array()
        .ok_or_else(|| {
            // If the top-level value has a "results" key, try that instead.
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Unexpected ranked results format.".to_string(),
                }),
            )
        })
        .or_else(|_| {
            json.get("results")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "Unexpected ranked results format.".to_string(),
                        }),
                    )
                })
        })?;

    let society = results
        .iter()
        .find(|item| {
            item.get("slug")
                .and_then(|v| v.as_str())
                .map(|id| id == slug)
                .unwrap_or(false)
        })
        .cloned()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "society_not_found".to_string(),
                }),
            )
        })?;

    Ok(Json(society))
}
