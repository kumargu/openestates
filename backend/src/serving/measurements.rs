//! Normalize observed measurement text offline; runtime only compares typed values.
use super::ServingFactRecord;
use crate::knowledge::FactValue;

/// Configured distance facts are kilometres. Preserve raw text for display only.
pub fn normalize_distance_fact(mut fact: ServingFactRecord) -> ServingFactRecord {
    if !crate::dag_config::nearby_place_categories_config()
        .categories
        .iter()
        .any(|category| category.fact_key == fact.fact_key)
    {
        return fact;
    }
    let text = match &fact.value {
        FactValue::Text(text) => text,
        _ => return fact,
    };
    if let Some(distance) = extract_unique_distance_km(text).filter(|v| v.is_finite() && *v >= 0.0)
    {
        fact.value_text = Some(text.clone());
        fact.value = FactValue::Numeric(distance);
        fact.value_type = "numeric".to_string();
    }
    fact
}

fn extract_unique_distance_km(text: &str) -> Option<f64> {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    let mut distances = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let cleaned = token.trim_matches(['(', ')', '[', ']', ',', ';']);
        let compact = cleaned
            .find(|ch: char| ch.is_ascii_alphabetic())
            .and_then(|start| {
                let (number, unit) = cleaned.split_at(start);
                let multiplier =
                    crate::search::parser::distance_unit_multiplier(&unit.to_ascii_lowercase())?;
                Some(number.parse::<f64>().ok()? * multiplier)
            });
        let separated = (index > 0)
            .then(|| {
                let multiplier =
                    crate::search::parser::distance_unit_multiplier(&cleaned.to_ascii_lowercase())?;
                Some(
                    tokens[index - 1]
                        .trim_matches(['(', ')', '[', ']', ',', ';'])
                        .parse::<f64>()
                        .ok()?
                        * multiplier,
                )
            })
            .flatten();
        if let Some(value) = compact.or(separated) {
            distances.push(value);
        }
    }
    (distances.len() == 1)
        .then(|| distances[0])
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_measurements_require_one_nonnegative_distance_with_a_unit() {
        assert_eq!(
            extract_unique_distance_km("Nearby metro: Example (0.7 km, 4.5 rating, 1509 reviews)"),
            Some(0.7)
        );
        assert_eq!(
            extract_unique_distance_km("Nearby school: Example (850m, 4.2 rating)"),
            Some(0.85)
        );
        for text in [
            "4.5 rating, 1509 reviews",
            "-1 km",
            "-850m",
            "School (1 km); School (2 km)",
        ] {
            assert_eq!(extract_unique_distance_km(text), None, "{text}");
        }
    }
}
