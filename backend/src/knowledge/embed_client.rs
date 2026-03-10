//! Lightweight client to embed text via Google gemini-embedding-001 API.
//! Used at search time to embed the user's query for semantic matching.
//!
//! Returns `Option` — embedding failure is non-fatal, search degrades gracefully.

use serde::Deserialize;

pub struct EmbedClient {
    api_key: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embedding: Option<EmbeddingValues>,
}

#[derive(Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}

impl EmbedClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
        }
    }

    /// Embed a text string into a 768-dim vector via Gemini embedding API.
    /// Returns None if the API call fails (search continues without semantic boost).
    pub async fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-001:embedContent?key={}",
            self.api_key
        );

        let body = serde_json::json!({
            "content": {
                "parts": [{"text": text}]
            }
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| {
                eprintln!("WARN: Embed API request failed: {}", e);
                e
            })
            .ok()?;

        if !resp.status().is_success() {
            let status = resp.status();
            eprintln!("WARN: Embed API returned {}", status);
            return None;
        }

        let parsed: EmbedResponse = resp.json().await.ok()?;
        let values = parsed.embedding?.values;

        if values.is_empty() {
            return None;
        }

        Some(values)
    }
}
