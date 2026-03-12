//! Gemini client for live property discovery.
//!
//! Makes a single HTTP call to Gemini 2.5 Flash with Google Search grounding
//! to discover real properties near a location. Same prompt structure as
//! pipeline/skills/discover_properties.py.

use serde::{Deserialize, Serialize};

/// Client for calling Gemini API.
pub struct GeminiClient {
    api_key: String,
    http_client: reqwest::Client,
}

/// A property discovered via Gemini + Google Search grounding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredProperty {
    pub project_name: String,
    pub builder_name: String,
    #[serde(default)]
    pub society_name: Option<String>,
    pub area: String,
    #[serde(default)]
    pub sub_area: Option<String>,
    #[serde(default)]
    pub configurations: Vec<String>,
    #[serde(default)]
    pub price_range_min_lakhs: Option<f64>,
    #[serde(default)]
    pub price_range_max_lakhs: Option<f64>,
    #[serde(default)]
    pub price_per_sqft_approx: Option<u64>,
    #[serde(default)]
    pub carpet_area_range: Option<String>,
    #[serde(default)]
    pub total_floors: Option<u32>,
    #[serde(default)]
    pub total_units_approx: Option<u32>,
    #[serde(default)]
    pub possession_status: Option<String>,
    #[serde(default)]
    pub year_built_or_expected: Option<u32>,
    #[serde(default)]
    pub distance_from_location_km: Option<f64>,
    #[serde(default)]
    pub rera_number: Option<String>,
    #[serde(default)]
    pub key_highlights: Vec<String>,
    #[serde(default)]
    pub source_url: Option<String>,
}

/// Response from Gemini discovery call.
#[derive(Debug, Deserialize)]
struct GeminiDiscoveryResponse {
    #[serde(default)]
    properties: Vec<DiscoveredProperty>,
    #[serde(default)]
    area_identified: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    nearby_landmarks: Vec<String>,
}

/// Raw Gemini API response structure.
#[derive(Debug, Deserialize)]
struct GeminiApiResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[allow(dead_code)]
    #[serde(default, rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiContent {
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

/// Constraints passed to the discovery prompt.
pub struct DiscoveryConstraints {
    pub bhk: Option<u32>,
    pub budget_max: Option<u64>,
}

impl GeminiClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            http_client: reqwest::Client::new(),
        }
    }

    /// Discover properties near a location using Gemini + Google Search grounding.
    /// Returns discovered properties and the area Gemini identified.
    pub async fn discover_properties(
        &self,
        area: &str,
        city: &str,
        constraints: &DiscoveryConstraints,
    ) -> Result<(Vec<DiscoveredProperty>, Option<String>), String> {
        let prompt = build_prompt(area, city, constraints);

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
            self.api_key
        );

        let body = serde_json::json!({
            "contents": [{"parts": [{"text": prompt}]}],
            "tools": [{"google_search": {}}],
            "generationConfig": {
                "temperature": 0.1,
                "maxOutputTokens": 16384,
            },
        });

        let resp = self
            .http_client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(90))
            .send()
            .await
            .map_err(|e| format!("Gemini API request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini API returned {}: {}", status, &body[..body.len().min(500)]));
        }

        let api_resp: GeminiApiResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

        // Extract text from multipart response (thinking + answer)
        let text_parts: Vec<String> = api_resp
            .candidates
            .into_iter()
            .flat_map(|c| c.content.into_iter())
            .flat_map(|c| c.parts.into_iter())
            .filter_map(|p| p.text)
            .collect();

        if text_parts.is_empty() {
            return Err("Empty response from Gemini".to_string());
        }

        // Try each part in reverse order (answer is usually last)
        for raw_text in text_parts.iter().rev() {
            if let Some(parsed) = try_parse_discovery_json(raw_text) {
                let area_identified = parsed.area_identified;
                let properties: Vec<DiscoveredProperty> = parsed
                    .properties
                    .into_iter()
                    .filter(|p| !p.project_name.is_empty() && !p.builder_name.is_empty())
                    .take(8) // Cap at 8
                    .collect();

                if !properties.is_empty() {
                    return Ok((properties, area_identified));
                }
            }
        }

        Err("No valid properties found in Gemini response".to_string())
    }
}

/// Build the discovery prompt — mirrors pipeline/skills/discover_properties.py.
fn build_prompt(area: &str, city: &str, constraints: &DiscoveryConstraints) -> String {
    let mut constraint_text = String::new();
    if let Some(bhk) = constraints.bhk {
        constraint_text.push_str(&format!("\nFocus on {} BHK configurations.", bhk));
    }
    if let Some(budget) = constraints.budget_max {
        let cr = budget as f64 / 10_000_000.0;
        if cr >= 1.0 {
            constraint_text.push_str(&format!("\nBudget: under {:.1} Cr.", cr));
        } else {
            let lakhs = budget as f64 / 100_000.0;
            constraint_text.push_str(&format!("\nBudget: under {:.0} Lakhs.", lakhs));
        }
    }

    format!(
        r#"Find real residential properties (apartments/flats) for sale near {area}, {city}.
Search for actual projects currently available or recently launched.
{constraint_text}

Return a JSON object with this structure:

{{
  "properties": [
    {{
      "project_name": "actual project name",
      "builder_name": "actual builder/developer name",
      "society_name": "society/complex name if different from project",
      "area": "micro-market / locality name",
      "sub_area": "specific sub-locality if known",
      "configurations": ["2 BHK", "3 BHK", "4 BHK"],
      "price_range_min_lakhs": 60,
      "price_range_max_lakhs": 150,
      "price_per_sqft_approx": 8500,
      "carpet_area_range": "1100-1800 sqft",
      "total_floors": 15,
      "total_units_approx": 500,
      "possession_status": "ready_to_move | under_construction | new_launch",
      "year_built_or_expected": 2024,
      "distance_from_location_km": 1.5,
      "rera_number": "RERA number if known, null otherwise",
      "key_highlights": ["2 sentences about what makes this project notable"],
      "source_url": "URL where you found this info"
    }}
  ],
  "area_identified": "canonical area name (e.g., Whitefield)",
  "nearby_landmarks": ["metro station", "IT park"]
}}

IMPORTANT:
- Return ONLY real, verifiable projects — no made-up data
- Include 5-8 properties (no more than 8)
- Price in lakhs (1 lakh = 100,000 INR)
- Return ONLY the JSON object, no markdown or explanation
- Use null for fields you can't verify
- Focus on projects within ~3-5 km of {area}"#,
        area = area,
        city = city,
        constraint_text = constraint_text,
    )
}

/// Try to parse discovery JSON from raw Gemini text.
/// Handles markdown wrapping and multi-part responses.
fn try_parse_discovery_json(raw: &str) -> Option<GeminiDiscoveryResponse> {
    let text = strip_markdown_fences(raw);

    // Try direct parse first
    if let Ok(parsed) = serde_json::from_str::<GeminiDiscoveryResponse>(&text) {
        return Some(parsed);
    }

    // Find outermost JSON object using brace counting
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;

    for i in start..bytes.len() {
        let c = bytes[i];
        if escape {
            escape = false;
            continue;
        }
        if c == b'\\' && in_string {
            escape = true;
            continue;
        }
        if c == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                let json_str = &text[start..=i];
                if let Ok(parsed) = serde_json::from_str::<GeminiDiscoveryResponse>(json_str) {
                    return Some(parsed);
                }
                break;
            }
        }
    }

    None
}

/// Strip markdown code fences from Gemini output.
fn strip_markdown_fences(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        let start = 1; // Skip first line (```json or ```)
        let end = if lines.last().is_some_and(|l| l.trim() == "```") {
            lines.len() - 1
        } else {
            lines.len()
        };
        lines[start..end].join("\n")
    } else {
        trimmed.to_string()
    }
}

/// Convert a string to a URL-safe slug.
pub fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
