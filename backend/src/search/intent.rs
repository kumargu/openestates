use serde::{Deserialize, Serialize};

use super::schema;

/// Parsed intent from a natural-language search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIntent {
    pub area: Option<String>,
    pub bhk: Option<u32>,
    pub budget_max: Option<u64>,
    /// Evidence-backed constraints that must be proven by structured/local facts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hard_constraints: Vec<HardConstraint>,
    /// Backward-compatible display list for the frontend.
    pub preferences: Vec<String>,
    /// Preferences the buyer wants to optimize for.
    #[serde(default)]
    pub positive_preferences: Vec<PreferenceSignal>,
    /// Preferences the buyer explicitly wants to avoid.
    #[serde(default)]
    pub negative_preferences: Vec<PreferenceSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer_archetype: Option<BuyerArchetype>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardConstraint {
    /// Registry dimension, e.g. "land_area".
    pub field: String,
    pub operator: ConstraintOperator,
    pub value: f64,
    pub unit: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOperator {
    Min,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceSignal {
    pub raw_text: String,
    pub polarity: Polarity,
    pub expanded_keys: Vec<String>,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuyerArchetype {
    Family,
    Investor,
    RiskAverse,
    ValueBuyer,
    LuxuryBuyer,
    EndUser,
}

/// Known area names and their aliases.
/// Includes landmark/station names that map to the canonical area.
///
/// NOTE: Currently Bengaluru-specific. For multi-city support, this should be
/// restructured into a per-city map (e.g. HashMap<City, Vec<(aliases, canonical)>>)
/// loaded from configuration rather than hardcoded. See also: extract_area_from_text
/// in routes/registration.rs which depends on this list.
pub const AREA_ALIASES: &[(&[&str], &str)] = &[
    (
        &[
            "whitefield",
            "wf",
            "kadugodi",
            "varthur",
            "itpl",
            "hope farm",
            "kundalahalli",
            "pattandur agrahara",
            "brookefield",
            "nallurhalli",
            "hagadur",
        ],
        "Whitefield",
    ),
    (
        &[
            "sarjapur",
            "sarjapur road",
            "sjr",
            "doddakannelli",
            "carmelaram",
        ],
        "Sarjapur Road",
    ),
    (
        &["bellandur", "outer ring road", "orr bellandur"],
        "Bellandur",
    ),
    (
        &["hsr", "hsr layout", "agara", "sector 1 hsr", "sector 2 hsr"],
        "HSR Layout",
    ),
    (
        &[
            "north bangalore",
            "north bengaluru",
            "north blr",
            "devanahalli",
            "hebbal",
            "yelahanka",
            "thanisandra",
            "jakkur",
        ],
        "North Bengaluru",
    ),
    (&["electronic city", "ec", "ecity"], "Electronic City"),
    (&["koramangala", "koramangala 5th block"], "Koramangala"),
    (&["marathahalli", "marathon halli"], "Marathahalli"),
    (&["indiranagar", "indira nagar"], "Indiranagar"),
    (&["jayanagar", "jaya nagar"], "Jayanagar"),
    (
        &["bannerghatta", "bannerghatta road", "btm"],
        "Bannerghatta Road",
    ),
];

/// Preference keywords to detect in the query.
///
/// IMPORTANT: Order matters — longer/more specific patterns must come BEFORE
/// shorter ones. "under construction" must match before "new" would catch it.
/// "ready to move" must match before "ready" alone.
///
/// The answers_preferences values here align with classify_project_status.py's
/// STATUS_META so that graph-driven scoring connects user queries to RERA facts.
const POSITIVE_PREFERENCE_PATTERNS: &[(&[&str], &str, &[&str], f32)] = &[
    // Project status patterns (specific phrases first)
    (
        &[
            "ready to move",
            "ready possession",
            "immediate possession",
            "delivered",
            "completed",
        ],
        "ready to move",
        &["possession_status", "project_status", "rera_status"],
        1.2,
    ),
    (
        &["under construction", "ongoing", "in progress"],
        "under construction",
        &["possession_status", "project_status"],
        1.0,
    ),
    (
        &["new launch", "newly launched", "just launched"],
        "new launch",
        &["possession_status", "project_status"],
        0.9,
    ),
    (
        &["delayed", "behind schedule"],
        "delayed",
        &["rera_delay_months", "possession_delay", "possession_status"],
        0.8,
    ),
    (
        &["upcoming", "pre-launch", "future project"],
        "upcoming",
        &["possession_status", "project_status"],
        0.8,
    ),
    // Builder trust patterns
    (
        &["reliable builder", "dependable builder"],
        "reliable builder",
        &[
            "builder_quality_score",
            "builder_reputation",
            "rera_builder_projects_count",
            "rera_builder_revocations",
            "delivery_track_record",
        ],
        1.3,
    ),
    (
        &["trusted builder", "good builder", "reputed builder"],
        "trusted builder",
        &[
            "builder_quality_score",
            "builder_reputation",
            "rera_builder_projects_count",
            "rera_builder_revocations",
        ],
        1.2,
    ),
    (
        &["on time delivery", "no delays", "timely delivery"],
        "on time delivery",
        &[
            "rera_delay_months",
            "builder_delivery_rate",
            "delivery_track_record",
            "possession_status",
        ],
        1.2,
    ),
    // General preferences
    (
        &["near metro", "metro access", "metro"],
        "metro access",
        &[
            "metro_distance_mins",
            "metro_distance",
            "metro_status",
            "metro_details",
            "metro_access",
        ],
        1.1,
    ),
    (
        &["quiet", "peaceful", "calm"],
        "quiet neighborhood",
        &["noise_score", "noise_level", "airport_noise_score"],
        1.1,
    ),
    (
        &["value", "affordable", "budget"],
        "value for money",
        &["price_per_sqft", "value_for_money", "maintenance_cost"],
        1.0,
    ),
    (
        &["premium", "luxury", "high end", "high-end"],
        "premium",
        &[
            "price_per_sqft",
            "amenity_quality",
            "finish_quality",
            "builder_reputation",
        ],
        1.0,
    ),
    (
        &["good society", "well maintained", "well-maintained"],
        "good society",
        &[
            "society_quality_score",
            "maintenance_quality",
            "resident_sentiment",
        ],
        1.1,
    ),
    (
        &["green", "greenery", "park", "garden"],
        "greenery",
        &["greenery_score", "open_space_score", "green_cover"],
        0.8,
    ),
    (
        &["new construction"],
        "new construction",
        &["possession_status", "project_status"],
        0.8,
    ),
    (&["top floor", "high floor"], "high floor", &["floor"], 0.6),
    (&["east facing", "east"], "east facing", &["facing"], 0.5),
];

const NEGATIVE_PREFERENCE_PATTERNS: &[(&[&str], &str, &[&str], f32)] = &[
    (
        &[
            "avoid waterlogging",
            "no waterlogging",
            "low waterlogging",
            "not flood prone",
            "avoid flooding",
            "no flooding",
        ],
        "waterlogging risk",
        &[
            "waterlogging_risk_score",
            "waterlogging_risk",
            "waterlogging_detail",
            "flooding_risk",
        ],
        1.4,
    ),
    (
        &[
            "avoid traffic",
            "less traffic",
            "low traffic",
            "no traffic",
            "traffic free",
        ],
        "traffic",
        &[
            "traffic_score",
            "traffic_reality",
            "traffic",
            "commute_reality",
        ],
        1.3,
    ),
    (
        &["avoid noise", "less noise", "low noise", "no noise"],
        "noise",
        &["noise_score", "noise_level", "airport_noise_score"],
        1.0,
    ),
    (
        &[
            "avoid legal",
            "no legal issue",
            "no litigation",
            "low litigation",
            "clear title",
        ],
        "legal risk",
        &[
            "litigation_risk",
            "legal_risk",
            "rera_complaints",
            "land_litigation",
        ],
        1.5,
    ),
    (
        &[
            "avoid delayed",
            "not delayed",
            "no possession delay",
            "avoid possession delay",
        ],
        "delay risk",
        &["rera_delay_months", "possession_delay", "possession_status"],
        1.4,
    ),
];

/// Parse a natural-language search query into structured intent.
pub fn parse_intent(query: &str) -> SearchIntent {
    let q = query.to_lowercase();

    let area = detect_area(&q);
    let bhk = detect_bhk(&q);
    let budget_max = detect_budget(&q);
    let hard_constraints = detect_hard_constraints(&q);
    let positive_preferences = detect_positive_preferences(&q);
    let negative_preferences = detect_negative_preferences(&q);
    let buyer_archetype = detect_buyer_archetype(&q);
    let preferences = display_preferences(&positive_preferences, &negative_preferences);

    SearchIntent {
        area,
        bhk,
        budget_max,
        hard_constraints,
        preferences,
        positive_preferences,
        negative_preferences,
        buyer_archetype,
    }
}

fn detect_area(q: &str) -> Option<String> {
    // Check multi-word aliases first (longer matches take priority).
    let mut best: Option<(&str, usize)> = None;
    for (aliases, canonical) in AREA_ALIASES {
        for alias in *aliases {
            if q.contains(alias) {
                let len = alias.len();
                if best.is_none() || len > best.unwrap().1 {
                    best = Some((canonical, len));
                }
            }
        }
    }
    best.map(|(name, _)| name.to_string())
}

fn detect_bhk(q: &str) -> Option<u32> {
    // Match patterns like "3bhk", "3 bhk", "3-bhk", "3 BHK"
    let bytes = q.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            let digit = (b - b'0') as u32;
            if (1..=6).contains(&digit) {
                // Look ahead for "bhk" possibly with a separator
                let rest = &q[i + 1..];
                if rest.starts_with("bhk") || rest.starts_with(" bhk") || rest.starts_with("-bhk") {
                    return Some(digit);
                }
            }
        }
    }
    None
}

fn detect_budget(q: &str) -> Option<u64> {
    // Patterns: "under 1.5cr", "below 80L", "under 1cr", "budget 90 lakhs"
    let q = q.replace(',', "");
    let tokens: Vec<&str> = q.split_whitespace().collect();

    for i in 0..tokens.len() {
        let is_budget_prefix = matches!(
            tokens[i],
            "under" | "below" | "budget" | "max" | "within" | "upto"
        );
        if !is_budget_prefix {
            continue;
        }
        // Try to parse the next token(s) as amount
        if let Some(amount) = parse_amount(&tokens[i + 1..]) {
            return Some(amount);
        }
    }

    // Also try standalone patterns like "1.5cr" without prefix
    for token in &tokens {
        if let Some(amount) = parse_single_amount(token) {
            // Only use standalone if it looks like a budget (has cr/l/lakh suffix)
            if token.ends_with("cr")
                || token.ends_with("crore")
                || token.ends_with("crores")
                || token.ends_with('l')
                || token.ends_with("lakh")
                || token.ends_with("lakhs")
            {
                return Some(amount);
            }
        }
    }

    None
}

fn parse_amount(tokens: &[&str]) -> Option<u64> {
    if tokens.is_empty() {
        return None;
    }

    // Try "1.5 cr", "80 lakhs", "1.5cr"
    let first = tokens[0];

    // Case: "1.5cr" or "80L" (number + suffix in one token)
    if let Some(amount) = parse_single_amount(first) {
        return Some(amount);
    }

    // Case: "1.5 cr" or "80 lakhs" (number then suffix)
    if tokens.len() >= 2 {
        if let Ok(num) = first.parse::<f64>() {
            let suffix = tokens[1];
            if suffix.starts_with("cr") {
                return Some((num * 10_000_000.0) as u64);
            } else if suffix.starts_with("l") {
                return Some((num * 100_000.0) as u64);
            }
        }
    }

    None
}

fn parse_single_amount(token: &str) -> Option<u64> {
    // "1.5cr" -> 15_000_000, "80l" -> 8_000_000
    let token = token.trim();
    if token.len() < 2 {
        return None;
    }

    let (num_part, suffix) = if let Some(stripped) = token.strip_suffix("crores") {
        (stripped, "cr")
    } else if let Some(stripped) = token.strip_suffix("crore") {
        (stripped, "cr")
    } else if let Some(stripped) = token.strip_suffix("cr") {
        (stripped, "cr")
    } else if let Some(stripped) = token.strip_suffix("lakhs") {
        (stripped, "l")
    } else if let Some(stripped) = token.strip_suffix("lakh") {
        (stripped, "l")
    } else if let Some(stripped) = token.strip_suffix('l') {
        (stripped, "l")
    } else {
        return None;
    };

    let num: f64 = num_part.parse().ok()?;
    match suffix {
        "cr" => Some((num * 10_000_000.0) as u64),
        "l" => Some((num * 100_000.0) as u64),
        _ => None,
    }
}

fn detect_hard_constraints(q: &str) -> Vec<HardConstraint> {
    schema::detect_hard_constraints(q)
}

fn detect_positive_preferences(q: &str) -> Vec<PreferenceSignal> {
    let mut prefs = detect_preference_signals(q, POSITIVE_PREFERENCE_PATTERNS, Polarity::Positive);
    for pattern in schema::positive_preference_patterns() {
        if !pattern
            .patterns
            .iter()
            .any(|term| query_contains_pattern(q, term))
        {
            continue;
        }

        if let Some(existing) = prefs.iter_mut().find(|p| p.raw_text == pattern.label) {
            for key in &pattern.expanded_keys {
                if !existing
                    .expanded_keys
                    .iter()
                    .any(|existing| existing == key)
                {
                    existing.expanded_keys.push(key.clone());
                }
            }
            existing.weight = existing.weight.max(pattern.weight);
        } else {
            prefs.push(schema::schema_preference_signal(
                pattern,
                Polarity::Positive,
            ));
        }
    }
    prefs
}

fn detect_negative_preferences(q: &str) -> Vec<PreferenceSignal> {
    detect_preference_signals(q, NEGATIVE_PREFERENCE_PATTERNS, Polarity::Negative)
}

fn detect_preference_signals(
    q: &str,
    patterns_table: &[(&[&str], &str, &[&str], f32)],
    polarity: Polarity,
) -> Vec<PreferenceSignal> {
    let mut prefs: Vec<PreferenceSignal> = Vec::new();
    for (patterns, label, expanded_keys, weight) in patterns_table {
        for pattern in *patterns {
            if query_contains_pattern(q, pattern) {
                if !prefs.iter().any(|p| p.raw_text == *label) {
                    prefs.push(PreferenceSignal {
                        raw_text: label.to_string(),
                        polarity: polarity.clone(),
                        expanded_keys: expanded_keys.iter().map(|key| key.to_string()).collect(),
                        weight: *weight,
                    });
                }
                break;
            }
        }
    }
    prefs
}

fn display_preferences(
    positive_preferences: &[PreferenceSignal],
    negative_preferences: &[PreferenceSignal],
) -> Vec<String> {
    positive_preferences
        .iter()
        .map(|p| p.raw_text.clone())
        .chain(
            negative_preferences
                .iter()
                .map(|p| format!("avoid {}", p.raw_text)),
        )
        .collect()
}

fn detect_buyer_archetype(q: &str) -> Option<BuyerArchetype> {
    if contains_any(q, &["family", "kids", "children", "school"]) {
        Some(BuyerArchetype::Family)
    } else if contains_any(
        q,
        &["investment", "investor", "rental", "resale", "appreciation"],
    ) {
        Some(BuyerArchetype::Investor)
    } else if contains_any(
        q,
        &[
            "risk averse",
            "safe bet",
            "low risk",
            "verified",
            "legal clear",
        ],
    ) {
        Some(BuyerArchetype::RiskAverse)
    } else if contains_any(q, &["luxury", "premium", "high end", "high-end"]) {
        Some(BuyerArchetype::LuxuryBuyer)
    } else if contains_any(q, &["value", "affordable", "budget"]) {
        Some(BuyerArchetype::ValueBuyer)
    } else if contains_any(
        q,
        &["end use", "end-user", "self use", "own stay", "live in"],
    ) {
        Some(BuyerArchetype::EndUser)
    } else {
        None
    }
}

fn contains_any(q: &str, patterns: &[&str]) -> bool {
    patterns
        .iter()
        .any(|pattern| query_contains_pattern(q, pattern))
}

fn query_contains_pattern(q: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }

    let mut search_start = 0;
    while let Some(relative_pos) = q[search_start..].find(pattern) {
        let start = search_start + relative_pos;
        let end = start + pattern.len();
        let before_ok = q[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());
        let after_ok = q[end..]
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric());

        if before_ok && after_ok {
            return true;
        }

        search_start = end;
        if search_start >= q.len() {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bhk() {
        let intent = parse_intent("3bhk in whitefield");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
    }

    #[test]
    fn test_parse_budget() {
        let intent = parse_intent("under 1.5cr in bellandur");
        assert_eq!(intent.budget_max, Some(15_000_000));
        assert_eq!(intent.area.as_deref(), Some("Bellandur"));
    }

    #[test]
    fn test_parse_preferences() {
        let intent = parse_intent("quiet 2bhk near metro");
        assert_eq!(intent.bhk, Some(2));
        assert!(intent.preferences.contains(&"metro access".to_string()));
        assert!(intent
            .preferences
            .contains(&"quiet neighborhood".to_string()));
    }

    #[test]
    fn test_parse_budget_lakhs() {
        let intent = parse_intent("3 bhk below 80l");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(8_000_000));
    }

    #[test]
    fn test_parse_min_land_area_constraint() {
        let intent = parse_intent("3bhk with greenery in whitefield above 10 acres");
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.hard_constraints.len(), 1);
        let constraint = &intent.hard_constraints[0];
        assert_eq!(constraint.field, "land_area");
        assert_eq!(constraint.operator, ConstraintOperator::Min);
        assert_eq!(constraint.value, 10.0);
        assert_eq!(constraint.unit, "acres");
    }

    #[test]
    fn test_parse_plus_acres_as_min_land_area_constraint() {
        let intent = parse_intent("3bhk whitefield 10+ acres");
        assert_eq!(intent.hard_constraints.len(), 1);
        assert_eq!(intent.hard_constraints[0].value, 10.0);
    }

    #[test]
    fn test_plain_acres_without_min_operator_is_not_hard_constraint() {
        let intent = parse_intent("3bhk whitefield 10 acres");
        assert!(intent.hard_constraints.is_empty());
    }

    // --- Day 62: Project status preference extraction tests ---

    #[test]
    fn test_ready_to_move_preference() {
        let intent = parse_intent("ready to move in whitefield");
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' preference, got: {:?}",
            intent.preferences
        );
        // Should NOT also extract "new construction"
        assert!(
            !intent.preferences.contains(&"new construction".to_string()),
            "Should not extract 'new construction' for 'ready to move' query"
        );
    }

    #[test]
    fn test_under_construction_preference() {
        let intent = parse_intent("under construction sarjapur");
        assert_eq!(intent.area.as_deref(), Some("Sarjapur Road"));
        assert!(
            intent
                .preferences
                .contains(&"under construction".to_string()),
            "Expected 'under construction' preference, got: {:?}",
            intent.preferences
        );
        // Must NOT extract "new construction" — that was the old buggy behavior
        assert!(
            !intent.preferences.contains(&"new construction".to_string()),
            "Should not extract 'new construction' for 'under construction' query"
        );
    }

    #[test]
    fn test_new_launch_preference() {
        let intent = parse_intent("new launch 3bhk whitefield");
        assert!(
            intent.preferences.contains(&"new launch".to_string()),
            "Expected 'new launch' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_delayed_preference() {
        let intent = parse_intent("delayed projects in sarjapur");
        assert!(
            intent.preferences.contains(&"delayed".to_string()),
            "Expected 'delayed' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_upcoming_preference() {
        let intent = parse_intent("upcoming projects in whitefield");
        assert!(
            intent.preferences.contains(&"upcoming".to_string()),
            "Expected 'upcoming' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_immediate_possession_maps_to_ready_to_move() {
        let intent = parse_intent("immediate possession bellandur");
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' from 'immediate possession', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_completed_maps_to_ready_to_move() {
        let intent = parse_intent("completed projects hsr layout");
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' from 'completed', got: {:?}",
            intent.preferences
        );
    }

    // --- Day 63: Builder preference pattern tests ---

    #[test]
    fn test_reliable_builder_preference() {
        let intent = parse_intent("reliable builder whitefield");
        assert!(
            intent.preferences.contains(&"reliable builder".to_string()),
            "Expected 'reliable builder' preference, got: {:?}",
            intent.preferences
        );
        assert_eq!(intent.area.as_deref(), Some("Whitefield"));
    }

    #[test]
    fn test_trusted_builder_preference() {
        let intent = parse_intent("trusted builder sarjapur");
        assert!(
            intent.preferences.contains(&"trusted builder".to_string()),
            "Expected 'trusted builder' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_on_time_delivery_preference() {
        let intent = parse_intent("on time delivery 3bhk whitefield");
        assert!(
            intent.preferences.contains(&"on time delivery".to_string()),
            "Expected 'on time delivery' preference, got: {:?}",
            intent.preferences
        );
        assert_eq!(intent.bhk, Some(3));
    }

    #[test]
    fn test_good_builder_maps_to_trusted_builder() {
        let intent = parse_intent("good builder bellandur");
        assert!(
            intent.preferences.contains(&"trusted builder".to_string()),
            "Expected 'trusted builder' from 'good builder', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_no_delays_maps_to_on_time_delivery() {
        let intent = parse_intent("no delays whitefield");
        assert!(
            intent.preferences.contains(&"on time delivery".to_string()),
            "Expected 'on time delivery' from 'no delays', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_avoid_waterlogging_is_negative_preference() {
        let intent = parse_intent("3bhk whitefield avoid waterlogging");
        assert_eq!(intent.positive_preferences.len(), 0);
        assert_eq!(intent.negative_preferences.len(), 1);
        assert_eq!(intent.negative_preferences[0].raw_text, "waterlogging risk");
        assert_eq!(intent.negative_preferences[0].polarity, Polarity::Negative);
        assert!(
            intent
                .preferences
                .contains(&"avoid waterlogging risk".to_string()),
            "Display preferences should include avoid-pref, got {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_less_traffic_and_not_delayed_are_negative_preferences() {
        let intent = parse_intent("family 3bhk sarjapur less traffic not delayed");
        let negative: Vec<&str> = intent
            .negative_preferences
            .iter()
            .map(|pref| pref.raw_text.as_str())
            .collect();
        assert!(
            negative.contains(&"traffic"),
            "Expected traffic negative preference: {:?}",
            negative
        );
        assert!(
            negative.contains(&"delay risk"),
            "Expected delay risk negative preference: {:?}",
            negative
        );
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
    }
}
