use serde::{Deserialize, Serialize};

/// Parsed intent from a natural-language search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIntent {
    pub area: Option<String>,
    pub bhk: Option<u32>,
    pub budget_max: Option<u64>,
    pub preferences: Vec<String>,
}

/// Known area names and their aliases.
/// Includes landmark/station names that map to the canonical area.
///
/// NOTE: Currently Bengaluru-specific. For multi-city support, this should be
/// restructured into a per-city map (e.g. HashMap<City, Vec<(aliases, canonical)>>)
/// loaded from configuration rather than hardcoded. See also: extract_area_from_text
/// in routes/registration.rs which depends on this list.
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

/// Preference keywords to detect in the query.
const PREFERENCE_PATTERNS: &[(&[&str], &str)] = &[
    (&["near metro", "metro access", "metro"], "metro access"),
    (&["quiet", "peaceful", "calm"], "quiet neighborhood"),
    (&["value", "affordable", "budget"], "value for money"),
    (&["premium", "luxury", "high end", "high-end"], "premium"),
    (&["good society", "well maintained", "well-maintained"], "good society"),
    (&["green", "greenery", "park", "garden"], "greenery"),
    (&["new", "under construction", "upcoming"], "new construction"),
    (&["ready to move", "ready", "immediate"], "ready to move"),
    (&["top floor", "high floor"], "high floor"),
    (&["east facing", "east"], "east facing"),
];

/// Parse a natural-language search query into structured intent.
pub fn parse_intent(query: &str) -> SearchIntent {
    let q = query.to_lowercase();

    let area = detect_area(&q);
    let bhk = detect_bhk(&q);
    let budget_max = detect_budget(&q);
    let preferences = detect_preferences(&q);

    SearchIntent {
        area,
        bhk,
        budget_max,
        preferences,
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
    // Patterns: "under 1.5cr", "below 80L", "under 1cr", "budget 90 lakhs"
    let q = q.replace(',', "");
    let tokens: Vec<&str> = q.split_whitespace().collect();

    for i in 0..tokens.len() {
        let is_budget_prefix = matches!(tokens[i], "under" | "below" | "budget" | "max" | "within" | "upto");
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

fn detect_preferences(q: &str) -> Vec<String> {
    let mut prefs = Vec::new();
    for (patterns, label) in PREFERENCE_PATTERNS {
        for pattern in *patterns {
            if q.contains(pattern) {
                prefs.push(label.to_string());
                break;
            }
        }
    }
    prefs
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
        assert!(intent.preferences.contains(&"quiet neighborhood".to_string()));
    }

    #[test]
    fn test_parse_budget_lakhs() {
        let intent = parse_intent("3 bhk below 80l");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(8_000_000));
    }
}
