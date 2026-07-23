use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::property_summary::{
    build_property_summary, default_summary_model, CreatePropertySummaryJobRequest,
    PropertySummaryJobResponse,
};
use crate::state::AppState;

pub async fn create_summary_job(
    State(state): State<Arc<AppState>>,
    Path(property_id): Path<String>,
    Json(request): Json<CreatePropertySummaryJobRequest>,
) -> Result<Json<PropertySummaryJobResponse>, StatusCode> {
    let bundle = {
        let bundle = state.serving_bundle.read().await;
        bundle
            .as_ref()
            .cloned()
            .ok_or(StatusCode::SERVICE_UNAVAILABLE)?
    };
    let bundle_version = bundle.manifest.bundle_version.clone();
    let (response, created) = {
        let mut jobs = state.property_summary_jobs.write().await;
        jobs.create_or_get(
            &property_id,
            &bundle_version,
            request.summary_style.as_deref(),
        )
    };

    if created
        && matches!(
            response.status,
            crate::property_summary::PropertySummaryJobStatus::Pending
        )
    {
        let state = state.clone();
        let job_id = response.job_id.clone();
        tokio::spawn(async move {
            let properties = state.properties.read().await.clone();
            let model = default_summary_model();
            let result =
                build_property_summary(&property_id, &properties, &bundle, model.as_ref()).await;
            let mut jobs = state.property_summary_jobs.write().await;
            match result {
                Ok((paragraph, evidence_refs, model_id)) => {
                    jobs.complete_ready(&job_id, paragraph, evidence_refs, model_id);
                }
                Err(message) => {
                    jobs.complete_error(&job_id, message);
                }
            }
        });
    }

    Ok(Json(response))
}

pub async fn get_summary_job(
    State(state): State<Arc<AppState>>,
    Path((_property_id, job_id)): Path<(String, String)>,
) -> Result<Json<PropertySummaryJobResponse>, StatusCode> {
    let response = {
        let mut jobs = state.property_summary_jobs.write().await;
        jobs.get(&job_id).ok_or(StatusCode::NOT_FOUND)?
    };
    Ok(Json(response))
}
