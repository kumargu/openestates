use chrono::{DateTime, Utc};

use crate::knowledge::FactValue;

use super::{
    ServingEntityFactRows, ServingFactIndex, ServingFactRecord, ServingSearchMetadataRecord,
};

const GOOGLE_NEARBY_PLACES_SKILL_ID: &str = "fetch_google_nearby_places";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoogleReviewEvidence {
    pub rating: Option<f64>,
    pub review_count: Option<u32>,
    pub reviews_url: Option<String>,
}

impl GoogleReviewEvidence {
    pub fn is_empty(&self) -> bool {
        self.rating.is_none() && self.review_count.is_none() && self.reviews_url.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectStatusEvidence {
    pub status: Option<String>,
    pub display: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HomeStateEvidence {
    pub state: Option<String>,
    pub age_bucket: Option<String>,
    pub display: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedFact<T> {
    pub value: T,
    pub confidence: f32,
    pub source_type: String,
    pub source_url: Option<String>,
    pub learned_at: DateTime<Utc>,
}

pub struct SocietyFactProjection<'a> {
    rows: Vec<&'a ServingEntityFactRows>,
}

impl<'a> SocietyFactProjection<'a> {
    pub fn from_index(index: &'a ServingFactIndex, society_id: &str) -> Self {
        let rows = society_entity_id_candidates(society_id)
            .iter()
            .filter_map(|entity_id| index.entity(entity_id))
            .collect();
        Self { rows }
    }

    pub fn latest_text(&self, fact_key: &str) -> Option<ProjectedFact<String>> {
        self.latest_valid(fact_key, |value| match value {
            FactValue::Text(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
            _ => None,
        })
    }

    pub fn latest_numeric(&self, fact_key: &str) -> Option<ProjectedFact<f64>> {
        self.latest_valid(fact_key, |value| match value {
            FactValue::Numeric(value) if value.is_finite() => Some(*value),
            _ => None,
        })
    }

    pub fn latest_bool(&self, fact_key: &str) -> Option<ProjectedFact<bool>> {
        self.latest_valid(fact_key, |value| match value {
            FactValue::Bool(value) => Some(*value),
            _ => None,
        })
    }

    pub fn latest_learned_at_with_prefix(&self, fact_key_prefix: &str) -> Option<DateTime<Utc>> {
        self.rows
            .iter()
            .flat_map(|rows| rows.facts.iter())
            .filter(|fact| fact.fact_key.starts_with(fact_key_prefix))
            .map(|fact| fact.learned_at)
            .max()
    }

    pub fn latest_record(&self, fact_key: &str) -> Option<&'a ServingFactRecord> {
        self.rows
            .iter()
            .flat_map(|rows| rows.facts.iter())
            .filter(|fact| fact.fact_key == fact_key)
            .max_by_key(|fact| fact.learned_at)
    }

    pub fn records(&self, fact_key: &str) -> Vec<&'a ServingFactRecord> {
        let mut facts = self
            .rows
            .iter()
            .flat_map(|rows| rows.facts.iter())
            .filter(|fact| fact.fact_key == fact_key)
            .collect::<Vec<_>>();
        facts.sort_by(|left, right| {
            right
                .learned_at
                .cmp(&left.learned_at)
                .then_with(|| left.source_url.cmp(&right.source_url))
        });
        facts
    }

    pub fn search_metadata(&self, fact_key: &str) -> Option<&'a ServingSearchMetadataRecord> {
        self.rows
            .iter()
            .flat_map(|rows| rows.search_metadata.iter())
            .find(|metadata| metadata.fact_key == fact_key)
    }

    pub fn project_google_reviews(&self, fallback: GoogleReviewEvidence) -> GoogleReviewEvidence {
        let reviews_url = self
            .latest_valid_google_review_fact("google_reviews_url", |value| match value {
                FactValue::Text(value) if is_http_url(value.trim()) => {
                    Some(value.trim().to_string())
                }
                _ => None,
            })
            .map(|fact| fact.value)
            .or(fallback.reviews_url);
        let rating = self
            .latest_valid_google_review_fact("google_rating", |value| match value {
                FactValue::Numeric(value) if value.is_finite() && (0.0..=5.0).contains(value) => {
                    Some(*value)
                }
                _ => None,
            })
            .map(|fact| fact.value)
            .or(fallback.rating);
        let review_count = self
            .latest_valid_google_review_fact("google_review_count", |value| match value {
                FactValue::Numeric(value)
                    if value.is_finite() && *value >= 0.0 && *value <= u32::MAX as f64 =>
                {
                    Some(*value)
                }
                _ => None,
            })
            .map(|fact| fact.value)
            .map(|count| count.round() as u32)
            .or(fallback.review_count);

        GoogleReviewEvidence {
            rating,
            review_count,
            reviews_url,
        }
    }

    fn latest_valid_google_review_fact<T>(
        &self,
        fact_key: &str,
        parse: impl Fn(&FactValue) -> Option<T>,
    ) -> Option<ProjectedFact<T>> {
        self.latest_valid_where(fact_key, parse, |fact| !is_nearby_place_google_fact(fact))
    }

    pub fn project_status(
        &self,
        fallback_status: Option<String>,
        fallback_display: Option<String>,
    ) -> ProjectStatusEvidence {
        ProjectStatusEvidence {
            status: fallback_status,
            display: fallback_display,
        }
    }

    pub fn project_home_state(&self) -> HomeStateEvidence {
        let state = self.latest_text("home_state").map(|fact| fact.value);
        let age_bucket = self.latest_text("home_age_bucket").map(|fact| fact.value);
        let timeline = self
            .latest_text("home_timeline_state")
            .map(|fact| fact.value);
        let display =
            home_state_display(state.as_deref(), age_bucket.as_deref(), timeline.as_deref());

        HomeStateEvidence {
            state,
            age_bucket,
            display,
        }
    }

    fn latest_valid<T>(
        &self,
        fact_key: &str,
        parse: impl Fn(&FactValue) -> Option<T>,
    ) -> Option<ProjectedFact<T>> {
        self.latest_valid_where(fact_key, parse, |_| true)
    }

    fn latest_valid_where<T>(
        &self,
        fact_key: &str,
        parse: impl Fn(&FactValue) -> Option<T>,
        include: impl Fn(&ServingFactRecord) -> bool,
    ) -> Option<ProjectedFact<T>> {
        self.rows
            .iter()
            .flat_map(|rows| rows.facts.iter())
            .filter(|fact| fact.fact_key == fact_key)
            .filter(|fact| include(fact))
            .filter_map(|fact| parse(&fact.value).map(|value| (fact, value)))
            .max_by_key(|(fact, _)| fact.learned_at)
            .map(|(fact, value)| projected_fact(fact, value))
    }
}

fn is_nearby_place_google_fact(fact: &ServingFactRecord) -> bool {
    fact.skill_id.as_deref() == Some(GOOGLE_NEARBY_PLACES_SKILL_ID)
}

fn projected_fact<T>(fact: &ServingFactRecord, value: T) -> ProjectedFact<T> {
    ProjectedFact {
        value,
        confidence: fact.confidence,
        source_type: fact.source_type.clone(),
        source_url: fact.source_url.clone(),
        learned_at: fact.learned_at,
    }
}

fn society_entity_id_candidates(society_id: &str) -> Vec<String> {
    let raw = society_id.trim().to_lowercase().replace(['_', ' '], "-");
    let slug = raw
        .strip_prefix("society:")
        .or_else(|| raw.strip_prefix("soc-"))
        .unwrap_or(&raw);
    let canonical = format!("society:{slug}");

    if raw == canonical {
        vec![canonical]
    } else {
        vec![canonical, raw]
    }
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn home_state_display(
    state: Option<&str>,
    age_bucket: Option<&str>,
    timeline: Option<&str>,
) -> Option<String> {
    match state {
        Some("delivered") => match age_bucket {
            Some("newly delivered") => Some("Delivered".to_string()),
            Some(bucket) => Some(format!("Delivered · {bucket}")),
            None => Some("Delivered".to_string()),
        },
        Some("under_construction")
            if timeline.is_some_and(|value| value.eq_ignore_ascii_case("delayed")) =>
        {
            Some("Delayed".to_string())
        }
        Some("under_construction") => Some("Under construction".to_string()),
        Some("delayed") => Some("Delayed".to_string()),
        _ => age_bucket.map(|bucket| format!("Est. {bucket}")),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::serving::ServingSearchMetadataRecord;

    #[test]
    fn newer_valid_facts_override_legacy_fallback() {
        let index = index(vec![
            fact(
                "society:prestige-park-grove",
                "google_rating",
                FactValue::Numeric(4.1),
                1,
            ),
            fact(
                "society:prestige-park-grove",
                "google_rating",
                FactValue::Numeric(4.6),
                2,
            ),
            fact(
                "society:prestige-park-grove",
                "google_review_count",
                FactValue::Numeric(431.0),
                2,
            ),
            fact(
                "society:prestige-park-grove",
                "google_reviews_url",
                FactValue::Text("https://example.com/current".to_string()),
                2,
            ),
        ]);

        let projected = SocietyFactProjection::from_index(&index, "soc-prestige-park-grove")
            .project_google_reviews(GoogleReviewEvidence {
                rating: Some(3.0),
                review_count: Some(10),
                reviews_url: Some("https://example.com/legacy".to_string()),
            });

        assert_eq!(projected.rating, Some(4.6));
        assert_eq!(projected.review_count, Some(431));
        assert_eq!(
            projected.reviews_url.as_deref(),
            Some("https://example.com/current")
        );
    }

    #[test]
    fn invalid_latest_values_do_not_hide_older_valid_serving_facts() {
        let index = index(vec![
            fact(
                "society:sample",
                "google_rating",
                FactValue::Numeric(4.2),
                1,
            ),
            fact(
                "society:sample",
                "google_rating",
                FactValue::Numeric(9.0),
                2,
            ),
            fact(
                "society:sample",
                "google_reviews_url",
                FactValue::Text("https://example.com/serving".to_string()),
                1,
            ),
            fact(
                "society:sample",
                "google_reviews_url",
                FactValue::Text("not-a-url".to_string()),
                2,
            ),
        ]);

        let projected = SocietyFactProjection::from_index(&index, "sample").project_google_reviews(
            GoogleReviewEvidence {
                rating: Some(3.5),
                review_count: Some(12),
                reviews_url: Some("https://example.com/legacy".to_string()),
            },
        );

        assert_eq!(projected.rating, Some(4.2));
        assert_eq!(projected.review_count, Some(12));
        assert_eq!(
            projected.reviews_url.as_deref(),
            Some("https://example.com/serving")
        );
    }

    #[test]
    fn nearby_place_google_metrics_do_not_project_as_society_reviews() {
        let index = index(vec![
            fact_with_skill(
                "society:godrej-splendour",
                "google_rating",
                FactValue::Numeric(3.6),
                2,
                Some("fetch_google_review_links"),
            ),
            fact_with_skill(
                "society:godrej-splendour",
                "google_review_count",
                FactValue::Numeric(791.0),
                2,
                Some("fetch_google_review_links"),
            ),
            fact_with_skill(
                "society:godrej-splendour",
                "google_rating",
                FactValue::Numeric(4.9),
                3,
                Some(GOOGLE_NEARBY_PLACES_SKILL_ID),
            ),
            fact_with_skill(
                "society:godrej-splendour",
                "google_review_count",
                FactValue::Numeric(493.0),
                3,
                Some(GOOGLE_NEARBY_PLACES_SKILL_ID),
            ),
        ]);

        let projected = SocietyFactProjection::from_index(&index, "godrej-splendour")
            .project_google_reviews(GoogleReviewEvidence::default());

        assert_eq!(projected.rating, Some(3.6));
        assert_eq!(projected.review_count, Some(791));
    }

    #[test]
    fn home_state_projection_keeps_card_label_compact() {
        let index = index(vec![
            fact(
                "society:sample",
                "home_state",
                FactValue::Text("delivered".to_string()),
                1,
            ),
            fact(
                "society:sample",
                "home_age_bucket",
                FactValue::Text("5-10 yrs old".to_string()),
                1,
            ),
        ]);

        let projected = SocietyFactProjection::from_index(&index, "sample").project_home_state();

        assert_eq!(projected.state.as_deref(), Some("delivered"));
        assert_eq!(projected.age_bucket.as_deref(), Some("5-10 yrs old"));
        assert_eq!(
            projected.display.as_deref(),
            Some("Delivered · 5-10 yrs old")
        );
    }

    #[test]
    fn delayed_timeline_wins_home_state_projection() {
        let index = index(vec![
            fact(
                "society:sample",
                "home_state",
                FactValue::Text("under_construction".to_string()),
                1,
            ),
            fact(
                "society:sample",
                "home_timeline_state",
                FactValue::Text("delayed".to_string()),
                2,
            ),
        ]);

        let projected = SocietyFactProjection::from_index(&index, "sample").project_home_state();

        assert_eq!(projected.display.as_deref(), Some("Delayed"));
    }

    #[test]
    fn delivered_home_state_wins_historical_delay_projection() {
        let index = index(vec![
            fact(
                "society:sample",
                "home_state",
                FactValue::Text("delivered".to_string()),
                2,
            ),
            fact(
                "society:sample",
                "home_timeline_state",
                FactValue::Text("delayed".to_string()),
                1,
            ),
        ]);

        let projected = SocietyFactProjection::from_index(&index, "sample").project_home_state();

        assert_eq!(projected.display.as_deref(), Some("Delivered"));
    }

    fn index(facts: Vec<ServingFactRecord>) -> ServingFactIndex {
        ServingFactIndex::from_records(facts, Vec::<ServingSearchMetadataRecord>::new())
    }

    fn fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        learned_at_seconds: i64,
    ) -> ServingFactRecord {
        fact_with_skill(entity_id, fact_key, value, learned_at_seconds, None)
    }

    fn fact_with_skill(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        learned_at_seconds: i64,
        skill_id: Option<&str>,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "test".to_string(),
            value_text: None,
            value,
            confidence: 0.9,
            source_type: "Google".to_string(),
            source_url: None,
            model: None,
            skill_id: skill_id.map(str::to_string),
            learned_at: Utc.timestamp_opt(learned_at_seconds, 0).unwrap(),
        }
    }
}
