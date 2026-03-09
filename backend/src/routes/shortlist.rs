use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct ShortlistResponse {
    pub shortlist: Vec<String>,
}

/// GET /api/shortlist — stub endpoint.
pub async fn get_shortlist() -> Json<ShortlistResponse> {
    Json(ShortlistResponse {
        shortlist: vec!["prop_w_001".to_string(), "prop_h_002".to_string()],
    })
}
