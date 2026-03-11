use serde::{Deserialize, Serialize};

/// Polarity of a preference — positive (want) vs negative (avoid).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Polarity {
    Positive,
    Negative,
}

/// Buyer archetype detected from query signals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuyerArchetype {
    Family,
    Investor,
    EndUser,
    RiskAverse,
    ValueBuyer,
    LuxuryBuyer,
}

/// A structured preference signal extracted from a user query.
///
/// Maps raw user language → canonical key → expanded KG fact keys.
/// Polarity distinguishes "want X" from "avoid X".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceSignal {
    /// Raw text from the query, e.g. "family friendly"
    pub raw_text: String,
    /// Canonical preference key, e.g. "family_friendly"
    pub canonical_key: String,
    /// Expanded KG fact keys this preference maps to
    pub expanded_keys: Vec<String>,
    pub polarity: Polarity,
    /// Emphasis weight — 1.0 by default, higher if user emphasized ("really", "must")
    pub weight: f32,
}

/// Parsed intent from a natural-language search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIntent {
    pub area: Option<String>,
    pub bhk: Option<u32>,
    pub budget_max: Option<u64>,
    /// Flat preference list — kept for backward compatibility.
    pub preferences: Vec<String>,
    /// Structured positive preferences (what the user wants).
    pub positive_preferences: Vec<PreferenceSignal>,
    /// Structured negative preferences (what the user wants to avoid).
    pub negative_preferences: Vec<PreferenceSignal>,
    /// Detected buyer archetype, if recognizable.
    pub buyer_archetype: Option<BuyerArchetype>,
}

/// Known area names and their aliases.
pub const AREA_ALIASES: &[(&[&str], &str)] = &[
    (&["whitefield", "wf", "kadugodi", "varthur", "itpl", "hope farm", "kundalahalli", "pattandur agrahara", "brookefield", "nallurhalli", "hagadur"], "Whitefield"),
    (&["sarjapur", "sarjapur road", "sjr", "doddakannelli", "carmelaram"], "Sarjapur Road"),
    (&["bellandur", "outer ring road", "orr bellandur"], "Bellandur"),
    (&["hsr", "hsr layout", "agara", "sector 1 hsr", "sector 2 hsr"], "HSR Layout"),
    (&["north bangalore", "north bengaluru", "north blr", "devanahalli", "hebbal", "yelahanka", "thanisandra", "jakkur"], "North Bengaluru"),
    (&["electronic city", "ec", "ecity"], "Electronic City"),
    (&["koramangala", "koramangala 5th block"], "Koramangala"),
    (&["marathahalli", "marathon halli"], "Marathahalli"),
    (&["indiranagar", "indira nagar"], "Indiranagar"),
    (&["jayanagar", "jaya nagar"], "Jayanagar"),
    (&["bannerghatta", "bannerghatta road", "btm"], "Bannerghatta Road"),
];

/// Preference taxonomy: (match_patterns, canonical_key, expanded_kg_fact_keys)
///
/// User language → canonical key → KG fact keys for scoring.
static PREFERENCE_TAXONOMY: &[(&[&str], &str, &[&str])] = &[
    (&["family friendly", "family-friendly", "family", "kids", "children", "good for family", "child"],
     "family_friendly",
     &["family_friendly", "child_safety", "playground", "school_nearby", "calm_environment", "community_vibe"]),

    (&["open space", "open spaces", "breathing room", "less crowded", "not crowded", "low density", "spacious",
       "not too dense", "not too packed", "not packed", "less dense", "not too crowded",
       "crowded", "packed", "dense", "chaotic", "chaos", "cramped"],
     "open_space",
     &["open_space_score", "density", "greenery_score", "breathing_room"]),

    (&["water issues", "water problem", "water supply", "tanker", "borewell", "waterlogging", "flooding"],
     "water_issues",
     &["water_supply", "waterlogging_risk", "tanker_dependency", "borewell_status"]),

    (&["quiet", "peaceful", "calm", "serene", "noise", "noisy", "highway noise", "traffic noise",
       "less noise", "no noise", "sound pollution", "construction noise"],
     "quiet",
     &["noise_score", "traffic_score", "calm_environment"]),

    (&["maintenance", "well maintained", "well-maintained", "upkeep", "society management",
       "maintenance headache", "maintenance issue", "upkeep issue"],
     "good_maintenance",
     &["maintenance_quality", "maintenance_cost", "society_management"]),

    (&["builder trust", "trusted builder", "reliable builder", "builder reputation", "good builder", "shady builder",
       "builder track record", "builder history", "reputed builder", "builder delivery"],
     "builder_trust",
     &["builder_reputation", "delivery_track_record", "quality_perception"]),

    (&["commute", "metro access", "connectivity", "well connected", "near metro", "metro",
       "traffic", "traffic congestion", "avoid traffic", "less traffic", "road access"],
     "commute",
     &["metro_distance", "traffic_score", "road_quality"]),

    (&["investment", "resale", "rental", "appreciation", "returns", "roi"],
     "investment",
     &["resale_strength", "market_activity", "area_trend", "rental_yield"]),

    (&["legal", "rera", "safe documents", "documentation", "legal safety", "paperwork"],
     "legal_safety",
     &["rera_status", "document_completeness", "litigation_risk"]),

    (&["green", "greenery", "park", "garden", "nature", "trees"],
     "greenery",
     &["greenery_score", "open_space_score", "park_nearby"]),

    (&["value for money", "affordable", "budget", "economical"],
     "value_for_money",
     &["value_for_money", "price_per_sqft", "maintenance_cost"]),

    (&["premium", "luxury", "high end", "high-end", "branded", "prestigious"],
     "premium",
     &["builder_reputation", "amenity_quality", "finish_quality"]),

    (&["good society", "nice society", "reputed society"],
     "good_society",
     &["maintenance_quality", "community_vibe", "society_management", "amenity_quality"]),

    (&["livability", "livable", "easy to live", "daily life", "daily convenience",
       "easier to live", "comfortable", "everyday life", "day to day"],
     "livability",
     &["livability_score", "maintenance_quality", "community_vibe", "daily_convenience"]),

    (&["dust", "pollution", "air quality", "construction dust", "smoke", "clean air"],
     "air_quality",
     &["air_quality", "pollution_level", "construction_activity"]),

    (&["safety", "safe", "secure", "security", "gated", "cctv"],
     "safety",
     &["safety_rating", "crime_rate", "gated_community"]),
];

/// Negation context words that flip polarity to Negative.
static NEGATION_WORDS: &[&str] = &[
    "avoid", "no ", "not ", "don't", "dont", "without", "issue", "issues",
    "problem", "problems", "bad", "poor", "shady", "headache", "fake",
    "don't want", "want to avoid", "want no", "less ", "reduce ",
];

/// Emphasis words that increase preference weight to 1.5.
static EMPHASIS_WORDS: &[&str] = &[
    "really", "very", "must", "critical", "important", "top priority",
    "absolutely", "strongly", "definitely", "essential",
];

/// Archetype detection patterns — first strong match wins.
static ARCHETYPE_PATTERNS: &[(&[&str], BuyerArchetype)] = &[
    (&["family", "kids", "children", "school", "parents", "family friendly"], BuyerArchetype::Family),
    (&["investment", "returns", "resale", "rental yield", "appreciation", "investor"], BuyerArchetype::Investor),
    (&["safe", "legal", "rera", "risk-free", "risk free", "no risk", "reliable", "trusted", "paperwork"], BuyerArchetype::RiskAverse),
    (&["affordable", "budget", "economical", "value for money"], BuyerArchetype::ValueBuyer),
    (&["premium", "luxury", "high end", "prestige", "willing to pay"], BuyerArchetype::LuxuryBuyer),
];

/// Parse a natural-language search query into structured intent.
pub fn parse_intent(query: &str) -> SearchIntent {
    let q = query.to_lowercase();

    let area = detect_area(&q);
    let bhk = detect_bhk(&q);
    let budget_max = detect_budget(&q);
    let (positive_preferences, negative_preferences) = detect_structured_preferences(&q);
    let buyer_archetype = detect_archetype(&q);

    // Backward-compat flat list — union of all matched raw_text values
    let preferences: Vec<String> = positive_preferences
        .iter()
        .chain(negative_preferences.iter())
        .map(|p| p.raw_text.clone())
        .collect();

    SearchIntent {
        area,
        bhk,
        budget_max,
        preferences,
        positive_preferences,
        negative_preferences,
        buyer_archetype,
    }
}

/// Detect structured preferences with polarity from user query.
fn detect_structured_preferences(q: &str) -> (Vec<PreferenceSignal>, Vec<PreferenceSignal>) {
    let mut positives = Vec::new();
    let mut negatives = Vec::new();
    let mut seen_canonical: std::collections::HashSet<String> = Default::default();

    for (patterns, canonical_key, expanded_keys) in PREFERENCE_TAXONOMY {
        if seen_canonical.contains(*canonical_key) {
            continue;
        }
        for &pattern in *patterns {
            if !q.contains(pattern) {
                continue;
            }

            // Check negation context: look back 35 chars before the pattern
            let pos = q.find(pattern).unwrap_or(0);
            let context_start = pos.saturating_sub(35);
            let context = &q[context_start..pos];
            let negated = NEGATION_WORDS.iter().any(|neg| context.contains(neg));

            let weight = if EMPHASIS_WORDS.iter().any(|e| q.contains(e)) { 1.5 } else { 1.0 };

            let signal = PreferenceSignal {
                raw_text: pattern.to_string(),
                canonical_key: canonical_key.to_string(),
                expanded_keys: expanded_keys.iter().map(|s| s.to_string()).collect(),
                polarity: if negated { Polarity::Negative } else { Polarity::Positive },
                weight,
            };

            seen_canonical.insert(canonical_key.to_string());
            if negated { negatives.push(signal); } else { positives.push(signal); }
            break; // First matching pattern per taxonomy entry is enough
        }
    }

    (positives, negatives)
}

/// Detect buyer archetype — highest-signal match wins.
fn detect_archetype(q: &str) -> Option<BuyerArchetype> {
    let mut scores: Vec<(usize, &BuyerArchetype)> = ARCHETYPE_PATTERNS
        .iter()
        .filter_map(|(patterns, archetype)| {
            let count = patterns.iter().filter(|p| q.contains(**p)).count();
            if count > 0 { Some((count, archetype)) } else { None }
        })
        .collect();
    scores.sort_by_key(|(c, _)| std::cmp::Reverse(*c));
    scores.into_iter().next().map(|(_, a)| a.clone())
}

fn detect_area(q: &str) -> Option<String> {
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
    let bytes = q.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() {
            let digit = (b - b'0') as u32;
            if digit >= 1 && digit <= 6 {
                let rest = &q[i + 1..];
                if rest.starts_with("bhk")
                    || rest.starts_with(" bhk")
                    || rest.starts_with("-bhk")
                {
                    return Some(digit);
                }
            }
        }
    }
    None
}

fn detect_budget(q: &str) -> Option<u64> {
    let q = q.replace(',', "");
    let tokens: Vec<&str> = q.split_whitespace().collect();

    for i in 0..tokens.len() {
        let is_budget_prefix = matches!(tokens[i], "under" | "below" | "budget" | "max" | "within" | "upto");
        if !is_budget_prefix {
            continue;
        }
        if let Some(amount) = parse_amount(&tokens[i + 1..]) {
            return Some(amount);
        }
    }

    for token in &tokens {
        if let Some(amount) = parse_single_amount(token) {
            if token.ends_with("cr") || token.ends_with("crore") || token.ends_with("crores")
                || token.ends_with('l') || token.ends_with("lakh") || token.ends_with("lakhs")
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
    let first = tokens[0];
    if let Some(amount) = parse_single_amount(first) {
        return Some(amount);
    }
    if tokens.len() >= 2 {
        if let Ok(num) = first.parse::<f64>() {
            let suffix = tokens[1];
            if suffix.starts_with("cr") {
                return Some((num * 1_00_00_000.0) as u64);
            } else if suffix.starts_with("l") {
                return Some((num * 1_00_000.0) as u64);
            }
        }
    }
    None
}

fn parse_single_amount(token: &str) -> Option<u64> {
    let token = token.trim();
    if token.len() < 2 {
        return None;
    }
    let (num_part, suffix) = if token.ends_with("crores") {
        (&token[..token.len() - 6], "cr")
    } else if token.ends_with("crore") {
        (&token[..token.len() - 5], "cr")
    } else if token.ends_with("cr") {
        (&token[..token.len() - 2], "cr")
    } else if token.ends_with("lakhs") {
        (&token[..token.len() - 5], "l")
    } else if token.ends_with("lakh") {
        (&token[..token.len() - 4], "l")
    } else if token.ends_with('l') {
        (&token[..token.len() - 1], "l")
    } else {
        return None;
    };
    let num: f64 = num_part.parse().ok()?;
    match suffix {
        "cr" => Some((num * 1_00_00_000.0) as u64),
        "l" => Some((num * 1_00_000.0) as u64),
        _ => None,
    }
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
        assert_eq!(intent.budget_max, Some(1_50_00_000));
        assert_eq!(intent.area.as_deref(), Some("Bellandur"));
    }

    #[test]
    fn test_parse_budget_lakhs() {
        let intent = parse_intent("3 bhk below 80l");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(80_00_000));
    }

    #[test]
    fn test_positive_preferences() {
        let intent = parse_intent("quiet 2bhk near metro");
        assert_eq!(intent.bhk, Some(2));
        assert!(intent.positive_preferences.iter().any(|p| p.canonical_key == "commute"));
        assert!(intent.positive_preferences.iter().any(|p| p.canonical_key == "quiet"));
    }

    #[test]
    fn test_negative_preference() {
        let intent = parse_intent("avoid water issues in whitefield");
        assert!(!intent.negative_preferences.is_empty());
        let wp = intent.negative_preferences.iter().find(|p| p.canonical_key == "water_issues");
        assert!(wp.is_some());
        assert_eq!(wp.unwrap().polarity, Polarity::Negative);
    }

    #[test]
    fn test_buyer_archetype_family() {
        let intent = parse_intent("family friendly 3BHK Whitefield for kids");
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
    }

    #[test]
    fn test_buyer_archetype_investor() {
        let intent = parse_intent("best investment opportunity in whitefield under 1.5cr");
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Investor));
    }

    #[test]
    fn test_expanded_keys() {
        let intent = parse_intent("family friendly society whitefield");
        let ff = intent.positive_preferences.iter().find(|p| p.canonical_key == "family_friendly");
        assert!(ff.is_some());
        let ff = ff.unwrap();
        assert!(ff.expanded_keys.contains(&"child_safety".to_string()));
        assert!(ff.expanded_keys.contains(&"calm_environment".to_string()));
    }

    #[test]
    fn test_backward_compat_preferences() {
        let intent = parse_intent("quiet family friendly place");
        assert!(!intent.preferences.is_empty());
    }
}
