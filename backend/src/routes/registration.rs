use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::models::{AreaProfile, Property, RegistrationDraft, Seller};
use crate::search::intent::AREA_ALIASES;
use crate::state::AppState;

/// Max registration creations per rate-limit window (60 seconds).
const RATE_LIMIT_MAX: u32 = 30;
/// Max publish operations per rate-limit window (60 seconds) — tighter than creation.
const PUBLISH_RATE_LIMIT_MAX: u32 = 10;
/// Rate-limit window duration.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

#[derive(Serialize)]
pub struct RegistrationError {
    pub error: String,
}

#[derive(Serialize)]
pub struct RegistrationCreated {
    pub id: String,
    pub current_step: u8,
    pub completeness_pct: u32,
}

/// POST /api/registrations — create a blank registration draft.
pub async fn create_registration(
    State(state): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<RegistrationCreated>), (StatusCode, Json<RegistrationError>)> {
    // --- Rate limiting (reuse interest rate limiter pattern) ---
    {
        let mut limiter = state.registration_rate_limiter.write().await;
        let now = Instant::now();
        if now.duration_since(limiter.0) > RATE_LIMIT_WINDOW {
            *limiter = (now, 1);
        } else if limiter.1 >= RATE_LIMIT_MAX {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(RegistrationError {
                    error: "rate limit exceeded — try again shortly".to_string(),
                }),
            ));
        } else {
            limiter.1 += 1;
        }
    }

    // Generate draft ID: draft-{timestamp_millis}-{counter}
    let now = chrono::Utc::now();
    let timestamp_millis = now.timestamp_millis();
    let counter = state.registration_counter.fetch_add(1, Ordering::Relaxed);
    let draft_id = format!("draft-{}-{}", timestamp_millis, counter);

    let draft = RegistrationDraft::new(draft_id.clone());

    // Persist to data/registrations/{id}.json
    persist_draft(&state, &draft).await?;

    let pct = draft.completeness_pct();
    Ok((
        StatusCode::CREATED,
        Json(RegistrationCreated {
            id: draft.id,
            current_step: draft.current_step,
            completeness_pct: pct,
        }),
    ))
}

/// GET /api/registrations/{id} — load a draft for resume.
pub async fn get_registration(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RegistrationDraft>, (StatusCode, Json<RegistrationError>)> {
    load_draft(&state, &id).await.map(Json)
}

#[derive(Deserialize)]
pub struct Step1Payload {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
}

#[derive(Deserialize)]
pub struct Step2Payload {
    pub property_prompt: String,
}

#[derive(Deserialize, Serialize)]
pub struct Step3Payload {
    pub property_type: String,
    #[serde(default)]
    pub bhk: Option<u8>,
    #[serde(default)]
    pub carpet_area_sqft: Option<u32>,
    #[serde(default)]
    pub floor: Option<u8>,
    #[serde(default)]
    pub total_floors: Option<u8>,
    #[serde(default)]
    pub facing: Option<String>,
    #[serde(default)]
    pub furnishing: Option<String>,
    #[serde(default)]
    pub age_years: Option<u8>,
}

#[derive(Deserialize, Serialize)]
pub struct Step4Payload {
    pub asking_price: u64,
    #[serde(default)]
    pub price_negotiable: Option<bool>,
    #[serde(default)]
    pub maintenance_monthly: Option<u32>,
    #[serde(default)]
    pub possession_status: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct Step5Payload {
    #[serde(default)]
    pub has_sale_deed: Option<bool>,
    #[serde(default)]
    pub has_khata: Option<bool>,
    #[serde(default)]
    pub has_ec: Option<bool>,
    #[serde(default)]
    pub has_rera_registration: Option<bool>,
    #[serde(default)]
    pub rera_number: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct Step6Payload {
    #[serde(default)]
    pub photo_count: Option<u8>,
    #[serde(default)]
    pub has_floor_plan: Option<bool>,
    #[serde(default)]
    pub video_tour_url: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct Step7Payload {
    #[serde(default)]
    pub society_name: Option<String>,
    #[serde(default)]
    pub area: Option<String>,
    #[serde(default)]
    pub total_units: Option<u32>,
    #[serde(default)]
    pub amenities: Option<Vec<String>>,
    #[serde(default)]
    pub additional_notes: Option<String>,
}

#[derive(Serialize)]
pub struct StepUpdated {
    pub id: String,
    pub current_step: u8,
    pub completeness_pct: u32,
}

/// PUT /api/registrations/{id}/step/{step_num} — update a specific step.
pub async fn update_registration_step(
    State(state): State<Arc<AppState>>,
    Path((id, step_num)): Path<(String, u8)>,
    body: axum::body::Bytes,
) -> Result<Json<StepUpdated>, (StatusCode, Json<RegistrationError>)> {
    let mut draft = load_draft(&state, &id).await?;

    match step_num {
        1 => {
            let payload: Step1Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 1 payload: {}", e),
                    }),
                )
            })?;
            validate_step1(&payload)?;
            draft.name = Some(payload.name.trim().to_string());
            draft.email = payload
                .email
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from);
            draft.phone = payload
                .phone
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(String::from);
            if draft.current_step < 1 {
                draft.current_step = 1;
            }
        }
        2 => {
            let payload: Step2Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 2 payload: {}", e),
                    }),
                )
            })?;
            validate_step2(&payload)?;
            draft.property_prompt = Some(payload.property_prompt.trim().to_string());
            if draft.current_step < 2 {
                draft.current_step = 2;
            }
        }
        3 => {
            let payload: Step3Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 3 payload: {}", e),
                    }),
                )
            })?;
            validate_step3(&payload)?;
            draft.property_details = Some(serde_json::to_value(&payload).unwrap_or_default());
            if draft.current_step < 3 {
                draft.current_step = 3;
            }
        }
        4 => {
            let payload: Step4Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 4 payload: {}", e),
                    }),
                )
            })?;
            validate_step4(&payload)?;
            draft.pricing = Some(serde_json::to_value(&payload).unwrap_or_default());
            if draft.current_step < 4 {
                draft.current_step = 4;
            }
        }
        5 => {
            let payload: Step5Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 5 payload: {}", e),
                    }),
                )
            })?;
            validate_step5(&payload)?;
            draft.documents = Some(serde_json::to_value(&payload).unwrap_or_default());
            if draft.current_step < 5 {
                draft.current_step = 5;
            }
        }
        6 => {
            let payload: Step6Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 6 payload: {}", e),
                    }),
                )
            })?;
            validate_step6(&payload)?;
            draft.photos = Some(serde_json::to_value(&payload).unwrap_or_default());
            if draft.current_step < 6 {
                draft.current_step = 6;
            }
        }
        7 => {
            let payload: Step7Payload = serde_json::from_slice(&body).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: format!("invalid step 7 payload: {}", e),
                    }),
                )
            })?;
            validate_step7(&payload)?;
            draft.society_info = Some(serde_json::to_value(&payload).unwrap_or_default());
            if draft.current_step < 7 {
                draft.current_step = 7;
            }
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: format!("step must be 1-7, got {}", step_num),
                }),
            ));
        }
    }

    draft.updated_at = chrono::Utc::now().to_rfc3339();
    persist_draft(&state, &draft).await?;

    let pct = draft.completeness_pct();
    Ok(Json(StepUpdated {
        id: draft.id,
        current_step: draft.current_step,
        completeness_pct: pct,
    }))
}

// --- Validation helpers ---

fn validate_step1(payload: &Step1Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: "name is required".to_string(),
            }),
        ));
    }
    if name.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: "name must be 100 characters or fewer".to_string(),
            }),
        ));
    }

    if let Some(email) = &payload.email {
        let email = email.trim();
        if !email.is_empty() && (!email.contains('@') || !email.contains('.')) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "email must contain @ and .".to_string(),
                }),
            ));
        }
    }

    if let Some(phone) = &payload.phone {
        let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
        if !phone.trim().is_empty() && !(10..=15).contains(&digits.len()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "phone must be 10-15 digits".to_string(),
                }),
            ));
        }
    }

    Ok(())
}

fn validate_step2(payload: &Step2Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    let prompt = payload.property_prompt.trim();
    if prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: "property_prompt is required".to_string(),
            }),
        ));
    }
    if prompt.len() > 500 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: "property_prompt must be 500 characters or fewer".to_string(),
            }),
        ));
    }
    Ok(())
}

fn validate_step3(payload: &Step3Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    let valid_types = ["apartment", "villa", "plot", "independent_house"];
    if !valid_types.contains(&payload.property_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: format!("property_type must be one of: {}", valid_types.join(", ")),
            }),
        ));
    }

    // BHK required for apartment/villa
    if matches!(payload.property_type.as_str(), "apartment" | "villa") {
        match payload.bhk {
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: "bhk is required for apartment or villa".to_string(),
                    }),
                ));
            }
            Some(bhk) if !(1..=6).contains(&bhk) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: "bhk must be between 1 and 6".to_string(),
                    }),
                ));
            }
            _ => {}
        }
    }

    if let Some(area) = payload.carpet_area_sqft {
        if !(100..=50_000).contains(&area) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "carpet_area_sqft must be between 100 and 50000".to_string(),
                }),
            ));
        }
    }

    if let Some(floor) = payload.floor {
        if floor > 99 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "floor must be 0-99".to_string(),
                }),
            ));
        }
    }

    if let Some(total) = payload.total_floors {
        if !(1..=99).contains(&total) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "total_floors must be 1-99".to_string(),
                }),
            ));
        }
    }

    if let Some(ref facing) = payload.facing {
        let valid = [
            "north",
            "south",
            "east",
            "west",
            "north_east",
            "north_west",
            "south_east",
            "south_west",
        ];
        if !valid.contains(&facing.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: format!("facing must be one of: {}", valid.join(", ")),
                }),
            ));
        }
    }

    if let Some(ref furnishing) = payload.furnishing {
        let valid = ["furnished", "semi_furnished", "unfurnished"];
        if !valid.contains(&furnishing.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: format!("furnishing must be one of: {}", valid.join(", ")),
                }),
            ));
        }
    }

    if let Some(age) = payload.age_years {
        if age > 99 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "age_years must be 0-99".to_string(),
                }),
            ));
        }
    }

    Ok(())
}

fn validate_step4(payload: &Step4Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    if payload.asking_price < 100_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: "asking_price must be at least 100000 (1 lakh INR)".to_string(),
            }),
        ));
    }

    if let Some(maintenance) = payload.maintenance_monthly {
        if maintenance > 100_000 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "maintenance_monthly must be 100000 or less".to_string(),
                }),
            ));
        }
    }

    if let Some(ref status) = payload.possession_status {
        let valid = ["ready", "under_construction", "resale"];
        if !valid.contains(&status.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: format!("possession_status must be one of: {}", valid.join(", ")),
                }),
            ));
        }
    }

    Ok(())
}

fn validate_step5(payload: &Step5Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    if let Some(ref rera) = payload.rera_number {
        let trimmed = rera.trim();
        if trimmed.len() > 50 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "rera_number must be 50 characters or fewer".to_string(),
                }),
            ));
        }
        if !trimmed.is_empty()
            && !trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '/')
        {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "rera_number must be alphanumeric (hyphens and slashes allowed)"
                        .to_string(),
                }),
            ));
        }
    }
    Ok(())
}

fn validate_step6(payload: &Step6Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    if let Some(count) = payload.photo_count {
        if count > 20 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "photo_count must be 0-20".to_string(),
                }),
            ));
        }
    }
    if let Some(ref url) = payload.video_tour_url {
        if url.len() > 500 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "video_tour_url must be 500 characters or fewer".to_string(),
                }),
            ));
        }
    }
    Ok(())
}

fn validate_step7(payload: &Step7Payload) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    if let Some(ref name) = payload.society_name {
        if name.trim().len() > 100 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "society_name must be 100 characters or fewer".to_string(),
                }),
            ));
        }
    }
    if let Some(ref area) = payload.area {
        if area.trim().len() > 100 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "area must be 100 characters or fewer".to_string(),
                }),
            ));
        }
    }
    if let Some(units) = payload.total_units {
        if units == 0 || units > 10_000 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "total_units must be 1-10000".to_string(),
                }),
            ));
        }
    }
    if let Some(ref amenities) = payload.amenities {
        if amenities.len() > 20 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "amenities must have 20 items or fewer".to_string(),
                }),
            ));
        }
        for a in amenities {
            if a.len() > 50 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(RegistrationError {
                        error: "each amenity must be 50 characters or fewer".to_string(),
                    }),
                ));
            }
        }
    }
    if let Some(ref notes) = payload.additional_notes {
        if notes.len() > 500 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(RegistrationError {
                    error: "additional_notes must be 500 characters or fewer".to_string(),
                }),
            ));
        }
    }
    Ok(())
}

// --- Persistence helpers ---

fn registrations_dir(state: &AppState) -> std::path::PathBuf {
    state.project_root.join("data").join("registrations")
}

async fn persist_draft(
    state: &AppState,
    draft: &RegistrationDraft,
) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    let dir = registrations_dir(state);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationError {
                error: format!("failed to create registrations directory: {}", e),
            }),
        )
    })?;

    let json = serde_json::to_string_pretty(draft).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationError {
                error: format!("failed to serialize draft: {}", e),
            }),
        )
    })?;

    // Atomic write: tmp file + rename
    let file_path = dir.join(format!("{}.json", draft.id));
    let tmp_path = dir.join(format!("{}.json.tmp", draft.id));

    tokio::fs::write(&tmp_path, json.as_bytes())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegistrationError {
                    error: format!("failed to write draft: {}", e),
                }),
            )
        })?;

    tokio::fs::rename(&tmp_path, &file_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegistrationError {
                    error: format!("failed to rename draft: {}", e),
                }),
            )
        })?;

    Ok(())
}

async fn load_draft(
    state: &AppState,
    id: &str,
) -> Result<RegistrationDraft, (StatusCode, Json<RegistrationError>)> {
    let file_path = registrations_dir(state).join(format!("{}.json", id));
    let contents = tokio::fs::read_to_string(&file_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            (
                StatusCode::NOT_FOUND,
                Json(RegistrationError {
                    error: format!("registration '{}' not found", id),
                }),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegistrationError {
                    error: format!("failed to read draft: {}", e),
                }),
            )
        }
    })?;

    serde_json::from_str(&contents).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationError {
                error: format!("failed to parse draft: {}", e),
            }),
        )
    })
}

/// Fuzzy-match a user-provided society name against known societies.
///
/// Scoring approach: exact match gets highest score (u32::MAX), then longest
/// substring overlap (bidirectional), ties broken by specificity (shorter
/// canonical name = more specific).
///
/// Returns a reference to the best-matching Society, or None if no overlap found.
fn fuzzy_match_society<'a>(
    input: &str,
    societies: &'a [crate::models::Society],
) -> Option<&'a crate::models::Society> {
    let input_lower = input.to_lowercase();
    societies
        .iter()
        .filter_map(|s| {
            let s_lower = s.name.to_lowercase();

            // Exact match = highest score (u32::MAX)
            if s_lower == input_lower {
                return Some((s, u32::MAX));
            }

            // Score by longest common substring overlap (bidirectional)
            let overlap = if s_lower.contains(&input_lower) {
                input_lower.len()
            } else if input_lower.contains(&s_lower) {
                s_lower.len()
            } else {
                0
            };

            if overlap > 0 {
                // Prefer most specific match (shorter name = more specific when it fully contains input)
                // Use overlap as primary score, penalize longer names to break ties
                let specificity_bonus = 1000u32.saturating_sub(s_lower.len() as u32);
                Some((s, (overlap as u32) * 1000 + specificity_bonus))
            } else {
                None
            }
        })
        .max_by_key(|(_, score)| *score)
        .map(|(s, _)| s)
}

/// Extract area from free-text using AREA_ALIASES, falling back to state.areas name match.
/// Returns (canonical_area_name, area_id) if found.
fn extract_area_from_text(text: &str, areas: &[AreaProfile]) -> Option<(String, String)> {
    let text_lower = text.to_lowercase();

    // Scan AREA_ALIASES for the longest alias match (same strategy as detect_area in intent.rs)
    let mut best: Option<(&str, usize)> = None;
    for (aliases, canonical) in AREA_ALIASES {
        for alias in *aliases {
            if text_contains_phrase(&text_lower, alias) {
                let len = alias.len();
                if best.is_none() || len > best.unwrap().1 {
                    best = Some((canonical, len));
                }
            }
        }
    }

    if let Some((canonical_name, _)) = best {
        // Try to find matching area_id from state.areas
        let area_id = areas
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(canonical_name))
            .map(|a| a.id.clone())
            .unwrap_or_default();
        return Some((canonical_name.to_string(), area_id));
    }

    // Fallback: check against state.areas names for direct substring match
    for area in areas {
        if text_lower.contains(&area.name.to_lowercase()) {
            return Some((area.name.clone(), area.id.clone()));
        }
    }

    None
}

fn text_contains_phrase(text: &str, phrase: &str) -> bool {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return false;
    }

    let mut search_start = 0;
    while let Some(relative_pos) = text[search_start..].find(phrase) {
        let start = search_start + relative_pos;
        let end = start + phrase.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());

        if before_ok && after_ok {
            return true;
        }

        search_start = end;
        if search_start >= text.len() {
            return false;
        }
    }

    false
}

// --- Publish endpoint ---

#[derive(Serialize)]
pub struct PublishResult {
    pub seller_id: String,
    pub property_id: String,
    pub dashboard_url: String,
}

/// POST /api/registrations/{id}/publish — convert a completed draft into a real Seller + Property.
pub async fn publish_registration(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<PublishResult>), (StatusCode, Json<RegistrationError>)> {
    // --- Rate limiting (separate publish rate limiter, tighter than creation) ---
    {
        let mut limiter = state.publish_rate_limiter.write().await;
        let now = Instant::now();
        if now.duration_since(limiter.0) > RATE_LIMIT_WINDOW {
            *limiter = (now, 1);
        } else if limiter.1 >= PUBLISH_RATE_LIMIT_MAX {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(RegistrationError {
                    error: "publish rate limit exceeded — try again shortly".to_string(),
                }),
            ));
        } else {
            limiter.1 += 1;
        }
    }

    let mut draft = load_draft(&state, &id).await?;

    // Idempotency: reject if already published
    if let Some(ref existing_seller_id) = draft.published_seller_id {
        return Err((
            StatusCode::CONFLICT,
            Json(RegistrationError {
                error: format!(
                    "this registration has already been published as seller '{}'",
                    existing_seller_id
                ),
            }),
        ));
    }

    // Minimum viable: must have completed at least step 4 (pricing)
    if draft.current_step < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RegistrationError {
                error: format!(
                    "registration must complete at least step 4 (pricing) before publishing, current_step={}",
                    draft.current_step
                ),
            }),
        ));
    }

    let now = chrono::Utc::now();
    let timestamp_millis = now.timestamp_millis();
    let counter = state.registration_counter.fetch_add(1, Ordering::Relaxed);

    let seller_id = format!("seller-{}-{}", timestamp_millis, counter);
    let property_id = format!("prop-reg-{}-{}", timestamp_millis, counter);

    // Extract property details from step 3
    let (property_type, bhk, carpet_area, floor, total_floors, facing, _furnishing) =
        if let Some(ref details) = draft.property_details {
            let pd: Step3Payload =
                serde_json::from_value(details.clone()).unwrap_or(Step3Payload {
                    property_type: "apartment".to_string(),
                    bhk: None,
                    carpet_area_sqft: None,
                    floor: None,
                    total_floors: None,
                    facing: None,
                    furnishing: None,
                    age_years: None,
                });
            (
                pd.property_type,
                pd.bhk.unwrap_or(2) as u32,
                pd.carpet_area_sqft.unwrap_or(1000),
                pd.floor.unwrap_or(0) as u32,
                pd.total_floors.unwrap_or(1) as u32,
                pd.facing.unwrap_or_default(),
                pd.furnishing.unwrap_or_default(),
            )
        } else {
            (
                "apartment".to_string(),
                2,
                1000,
                0,
                1,
                String::new(),
                String::new(),
            )
        };

    // Extract pricing from step 4
    let (asking_price, maintenance_monthly, possession_status) =
        if let Some(ref pricing) = draft.pricing {
            let p: Step4Payload = serde_json::from_value(pricing.clone()).unwrap_or(Step4Payload {
                asking_price: 0,
                price_negotiable: None,
                maintenance_monthly: None,
                possession_status: None,
            });
            (
                p.asking_price,
                p.maintenance_monthly.unwrap_or(0),
                p.possession_status.unwrap_or_else(|| "ready".to_string()),
            )
        } else {
            (0, 0, "ready".to_string())
        };

    // Extract documents from step 5
    let documents_provided = if let Some(ref docs) = draft.documents {
        let d: Step5Payload = serde_json::from_value(docs.clone()).unwrap_or(Step5Payload {
            has_sale_deed: None,
            has_khata: None,
            has_ec: None,
            has_rera_registration: None,
            rera_number: None,
        });
        let mut provided = Vec::new();
        if d.has_sale_deed == Some(true) {
            provided.push("sale_deed".to_string());
        }
        if d.has_khata == Some(true) {
            provided.push("khata".to_string());
        }
        if d.has_ec == Some(true) {
            provided.push("ec".to_string());
        }
        if d.has_rera_registration == Some(true) {
            provided.push("rera".to_string());
        }
        provided
    } else {
        Vec::new()
    };

    // Extract society name and area from step 7
    let (society_name, seller_area) = if let Some(ref info) = draft.society_info {
        let sn = info
            .get("society_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Society")
            .to_string();
        let sa = info
            .get("area")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        (sn, sa)
    } else {
        ("Unknown Society".to_string(), None)
    };

    // Fuzzy-match society_name against known societies to populate society_id, area, area_id.
    // Fallback chain:
    //   1) matched society -> inherit area + area_id
    //   2) seller-provided area in step 7 -> use directly, match area_id
    //   3) property_prompt mentions a known area -> extract and use it
    //   4) empty (appears only in browse/keyword, not area search)
    let matched_society = fuzzy_match_society(&society_name, &state.societies);

    let (resolved_society_id, resolved_area, resolved_area_id, resolved_builder) =
        if let Some(soc) = matched_society {
            // Find area_id from state.areas by matching the society's area name
            let area_id = state
                .areas
                .iter()
                .find(|a| a.name.to_lowercase() == soc.area.to_lowercase())
                .map(|a| a.id.clone())
                .unwrap_or_default();
            (
                soc.id.clone(),
                soc.area.clone(),
                area_id,
                soc.builder_name.clone(),
            )
        } else if let Some(ref area_str) = seller_area {
            // No society match, but seller provided an area — try to match area_id
            let area_id = state
                .areas
                .iter()
                .find(|a| a.name.to_lowercase() == area_str.to_lowercase())
                .map(|a| a.id.clone())
                .unwrap_or_default();
            (String::new(), area_str.clone(), area_id, String::new())
        } else if let Some(ref prompt) = draft.property_prompt {
            // Fallback 3: extract area from property_prompt text
            if let Some((area_name, area_id)) = extract_area_from_text(prompt, &state.areas) {
                (String::new(), area_name, area_id, String::new())
            } else {
                (String::new(), String::new(), String::new(), String::new())
            }
        } else {
            (String::new(), String::new(), String::new(), String::new())
        };

    // DAG enrichment deferred to the offline pipeline — published properties start with
    // transparency_tags: ["seller-registered", "verification-pending"] and get enriched
    // by Python skills and promoted serving bundles per AGENTS.md.

    let seller_name = draft
        .name
        .clone()
        .unwrap_or_else(|| "Unknown Seller".to_string());

    // Auto-generate property title
    let title = format!(
        "{} BHK {} in {} by {}",
        bhk, property_type, society_name, seller_name
    );

    // Compute price_per_sqft
    let price_per_sqft = if carpet_area > 0 {
        asking_price / carpet_area as u64
    } else {
        0
    };

    let now_str = now.to_rfc3339();

    // Build Seller
    let seller = Seller {
        id: seller_id.clone(),
        name: seller_name,
        email: draft.email.clone(),
        phone: draft.phone.clone(),
        property_ids: vec![property_id.clone()],
        has_basic_info: draft.name.is_some(),
        has_property_prompt: draft.property_prompt.is_some(),
        property_prompt: draft.property_prompt.clone(),
        has_details: draft.property_details.is_some(),
        has_pricing: draft.pricing.is_some(),
        has_documents: draft.documents.is_some(),
        has_photos: draft.photos.is_some(),
        has_society_info: draft.society_info.is_some(),
        documents_provided,
        verified: false,
        created_at: now_str.clone(),
        updated_at: now_str.clone(),
    };

    // Build Property with sensible defaults for fields we don't have yet
    let property = Property {
        id: property_id.clone(),
        title,
        area: resolved_area,
        area_id: resolved_area_id,
        city: "bengaluru".to_string(),
        society_id: resolved_society_id,
        builder_name: resolved_builder,
        property_type: property_type.clone(),
        listing_type: "resale".to_string(),
        bhk,
        price: asking_price,
        price_per_sqft,
        carpet_area_sqft: carpet_area,
        super_builtup_sqft: 0,
        floor,
        total_floors,
        facing: facing.clone(),
        possession_status,
        metro_distance_mins: 0,
        maintenance_cost_monthly: maintenance_monthly,
        society_quality_score: 0.0,
        builder_quality_score: 0.0,
        document_completeness_score: 0.0,
        litigation_risk: 0.0,
        noise_score: 0.0,
        sunlight_score: 0.0,
        airport_noise_score: 0.0,
        waterlogging_risk_score: 0.0,
        traffic_score: 0.0,
        days_on_market: 0,
        greenery_score: None,
        open_space_score: None,
        resale_strength_score: None,
        interest_level: None,
        saves_last_7d: None,
        offers_last_7d: None,
        images: Vec::new(),
        hero_image: String::new(),
        description_summary: draft.property_prompt.clone().unwrap_or_default(),
        transparency_tags: vec![
            "seller-registered".to_string(),
            "verification-pending".to_string(),
        ],
        source_reference: format!("registration:{}", draft.id),
        seller_id: Some(seller_id.clone()),
    };

    // Safety net: if area is still empty but description_summary has content,
    // try to extract area from description_summary (which contains property_prompt).
    let property = if property.area.is_empty() && !property.description_summary.is_empty() {
        if let Some((area_name, area_id)) =
            extract_area_from_text(&property.description_summary, &state.areas)
        {
            Property {
                area: area_name,
                area_id,
                ..property
            }
        } else {
            property
        }
    } else {
        property
    };

    // --- Persist to disk (atomic writes) ---

    // 1. Append seller to data/sellers/sellers.json
    let sellers_dir = state.project_root.join("data").join("sellers");
    tokio::fs::create_dir_all(&sellers_dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationError {
                error: format!("failed to create sellers directory: {}", e),
            }),
        )
    })?;
    let sellers_path = sellers_dir.join("sellers.json");
    let mut all_sellers: Vec<Seller> = if sellers_path.exists() {
        let content = tokio::fs::read_to_string(&sellers_path)
            .await
            .unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    all_sellers.push(seller.clone());
    atomic_write_json(&sellers_path, &all_sellers).await?;

    // --- Insert into in-memory state ---
    {
        let mut sellers_lock = state.sellers.write().await;
        sellers_lock.push(seller);
    }
    {
        let mut properties_lock = state.properties.write().await;
        let mut search_index = state.search_index.write().await;
        search_index.insert(&property);
        properties_lock.push(property);
    }

    // --- Mark draft as published (idempotency) ---
    draft.published_seller_id = Some(seller_id.clone());
    draft.updated_at = now_str;
    persist_draft(&state, &draft).await?;

    let dashboard_url = format!("/seller/{}", seller_id);

    Ok((
        StatusCode::OK,
        Json(PublishResult {
            seller_id,
            property_id,
            dashboard_url,
        }),
    ))
}

/// Atomic write: serialize to JSON, write to .tmp, rename to final path.
async fn atomic_write_json<T: Serialize>(
    path: &std::path::Path,
    data: &T,
) -> Result<(), (StatusCode, Json<RegistrationError>)> {
    let json = serde_json::to_string_pretty(data).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationError {
                error: format!("failed to serialize: {}", e),
            }),
        )
    })?;

    let tmp_path = path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, json.as_bytes())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RegistrationError {
                    error: format!("failed to write tmp file: {}", e),
                }),
            )
        })?;

    tokio::fs::rename(&tmp_path, path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RegistrationError {
                error: format!("failed to rename: {}", e),
            }),
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        area_profile::{AreaProfile, PriceRange, RedditSignals},
        Society,
    };

    /// Helper to build a minimal Society for testing.
    fn test_society(id: &str, name: &str) -> Society {
        Society {
            id: id.to_string(),
            name: name.to_string(),
            area: "Whitefield".to_string(),
            city: "bengaluru".to_string(),
            builder_name: "Test Builder".to_string(),
            year_built: 2020,
            total_units: 100,
            summary: String::new(),
            maintenance_sentiment: String::new(),
            livability_sentiment: String::new(),
            common_positives: Vec::new(),
            common_complaints: Vec::new(),
            review_summary: String::new(),
            google_reviews_url: None,
            future_google_place_name: String::new(),
            future_google_place_id: None,
            future_review_enrichment_status: String::new(),
        }
    }

    /// Helper to build a minimal AreaProfile for testing.
    fn test_area(id: &str, name: &str) -> AreaProfile {
        AreaProfile {
            id: id.to_string(),
            name: name.to_string(),
            city: "bengaluru".to_string(),
            median_price_per_sqft: 8000,
            price_range_per_sqft: PriceRange {
                low: 6000,
                high: 12000,
            },
            trend_direction: String::new(),
            trend_summary: String::new(),
            metro_access_summary: String::new(),
            airport_noise_summary: String::new(),
            traffic_summary: String::new(),
            waterlogging_summary: String::new(),
            livability_summary: String::new(),
            externality_tags: Vec::new(),
            infrastructure_tags: Vec::new(),
            reddit_signals: RedditSignals {
                decision_drivers: Vec::new(),
                recurring_concerns: Vec::new(),
                sentiment_label: String::new(),
                last_updated: String::new(),
            },
            community_notes: String::new(),
            sample_size: 0,
            last_updated: String::new(),
        }
    }

    // --- Fuzzy match tests ---

    #[test]
    fn test_fuzzy_match_exact() {
        let societies = vec![
            test_society("s1", "Prestige Lakeside Habitat"),
            test_society("s2", "The Prestige City"),
        ];
        let result = fuzzy_match_society("Prestige Lakeside Habitat", &societies);
        assert_eq!(result.map(|s| s.id.as_str()), Some("s1"));
    }

    #[test]
    fn test_fuzzy_match_substring_prefers_longest() {
        // "Prestige Lakeside" should match "Prestige Lakeside Habitat" (overlap=17)
        // not "The Prestige City" (overlap=8 for "Prestige")
        let societies = vec![
            test_society("s1", "Prestige Lakeside Habitat"),
            test_society("s2", "The Prestige City"),
        ];
        let result = fuzzy_match_society("Prestige Lakeside", &societies);
        assert_eq!(result.map(|s| s.id.as_str()), Some("s1"));
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        let societies = vec![
            test_society("s1", "Prestige Lakeside Habitat"),
            test_society("s2", "Brigade Gateway"),
        ];
        let result = fuzzy_match_society("Sobha Dream Acres", &societies);
        assert!(result.is_none());
    }

    #[test]
    fn test_fuzzy_match_case_insensitive() {
        let societies = vec![test_society("s1", "Prestige Lakeside Habitat")];
        let result = fuzzy_match_society("prestige lakeside habitat", &societies);
        assert_eq!(result.map(|s| s.id.as_str()), Some("s1"));
    }

    // --- Area extraction tests ---

    #[test]
    fn test_extract_area_whitefield() {
        let areas = vec![test_area("area-whitefield", "Whitefield")];
        let result = extract_area_from_text("Beautiful 3BHK near ITPL Whitefield", &areas);
        assert!(result.is_some());
        let (area_name, _) = result.unwrap();
        assert_eq!(area_name, "Whitefield");
    }

    #[test]
    fn test_extract_area_sarjapur() {
        let areas = vec![test_area("area-sarjapur", "Sarjapur Road")];
        let result = extract_area_from_text("Corner flat in Sarjapur Road", &areas);
        assert!(result.is_some());
        let (area_name, _) = result.unwrap();
        assert_eq!(area_name, "Sarjapur Road");
    }

    #[test]
    fn test_extract_area_no_match() {
        let areas = vec![test_area("area-whitefield", "Whitefield")];
        let result = extract_area_from_text("Beautiful flat with sunrise views", &areas);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_area_via_alias() {
        // "ITPL" is an alias for Whitefield in AREA_ALIASES
        let areas = vec![test_area("area-whitefield", "Whitefield")];
        let result = extract_area_from_text("Office near ITPL", &areas);
        assert!(result.is_some());
        let (area_name, _) = result.unwrap();
        assert_eq!(area_name, "Whitefield");
    }

    #[test]
    fn test_extract_area_alias_does_not_match_inside_words() {
        // "ec" is an Electronic City alias and must not match the middle of "tech".
        let areas = vec![test_area("area-electronic-city", "Electronic City")];
        let result = extract_area_from_text("Office near tech parks", &areas);
        assert!(result.is_none());
    }
}
