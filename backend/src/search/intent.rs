use serde::{Deserialize, Serialize};

use super::query_plan;

/// Byte span in the original buyer query, preserved for safe query editing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub raw_text: String,
}

/// Parsed intent from a natural-language search query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchIntent {
    pub area: Option<String>,
    /// Broad regions explicitly rejected by the buyer, e.g. "not South Bengaluru".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_areas: Vec<String>,
    /// Named societies/projects explicitly rejected by the buyer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_societies: Vec<String>,
    /// Named builders explicitly rejected by the buyer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_builders: Vec<String>,
    /// Positive area alternatives. A home matches if it is in any of these areas.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub areas: Vec<String>,
    pub bhk: Option<u32>,
    /// Positive BHK alternatives. A home matches if its BHK is in this set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bhks: Vec<u32>,
    /// BHK values the buyer explicitly rejected, e.g. "not 4 BHK".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_bhks: Vec<u32>,
    /// Source spans for BHK clauses used by query-aware presentation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bhk_spans: Vec<SourceSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_min: Option<u64>,
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
    /// Explicit buyer ordering for soft preferences, highest priority first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ranking_priorities: Vec<String>,
    /// Risks the buyer says they can accept as a tradeoff, e.g. "bad traffic but great amenities".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_tradeoffs: Vec<String>,
    /// Inventory classes requested but not currently supported by the apartment corpus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_inventory_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buyer_archetype: Option<BuyerArchetype>,
}

impl SearchIntent {
    /// Areas the buyer will accept. Falls back to the single `area` slot.
    pub fn requested_areas(&self) -> Vec<&str> {
        if !self.areas.is_empty() {
            self.areas.iter().map(String::as_str).collect()
        } else {
            self.area.as_deref().into_iter().collect()
        }
    }

    /// BHK values the buyer will accept. Falls back to the single `bhk` slot.
    pub fn requested_bhks(&self) -> Vec<u32> {
        if !self.bhks.is_empty() {
            self.bhks.clone()
        } else {
            self.bhk.into_iter().collect()
        }
    }
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
    Max,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gap_keys: Vec<String>,
    pub weight: f32,
    /// A configured categorical requirement or an explicitly mandatory buyer clause.
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub missing_evidence_neutral: bool,
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

/// Parse a natural-language search query into structured intent.
pub fn parse_intent(query: &str) -> SearchIntent {
    let plan = query_plan::compile_query_plan(query);
    query_plan::project_search_intent(query, &plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bhk() {
        let intent = parse_intent("3bhk in east bangalore");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.bhks, vec![3]);
        assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
        assert_eq!(intent.areas, vec!["East Bengaluru".to_string()]);
    }

    #[test]
    fn bhk_alternatives_are_kept_as_a_set() {
        let intent = parse_intent("2 or 3 BHK in East Bengaluru");
        assert_eq!(intent.bhk, None);
        assert_eq!(intent.bhks, vec![2, 3]);
        assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
    }

    #[test]
    fn excluded_bhk_is_kept_off_the_include_set() {
        let intent = parse_intent("2 or 3 BHK, not 4 BHK");
        assert_eq!(intent.bhks, vec![2, 3]);
        assert_eq!(intent.exclude_bhks, vec![4]);
        assert_eq!(intent.bhk, None);
        assert_eq!(intent.bhk_spans.len(), 3);
        assert!(
            intent
                .preferences
                .iter()
                .all(|preference| { !preference.contains("bhk configuration") }),
            "parsed BHK clauses should not also score as configuration preferences: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn parsed_bhk_is_not_also_a_configuration_preference() {
        let intent = parse_intent("2 or 3 BHK in Whitefield");
        assert_eq!(intent.bhks, vec![2, 3]);
        assert!(
            !intent
                .preferences
                .iter()
                .any(|preference| preference.contains("bhk configuration")),
            "got {:?}",
            intent.preferences
        );
        let single = parse_intent("3BHK Whitefield under 2Cr");
        assert_eq!(single.bhk, Some(3));
        assert!(
            !single
                .preferences
                .iter()
                .any(|preference| preference.contains("bhk configuration")),
            "got {:?}",
            single.preferences
        );
    }

    #[test]
    fn area_alternatives_are_kept_as_a_set() {
        let intent = parse_intent("3BHK in East Bengaluru or South Bengaluru");
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area, None);
        assert_eq!(intent.areas.len(), 2);
        assert!(intent.areas.iter().any(|area| area == "East Bengaluru"));
        assert!(intent.areas.iter().any(|area| area == "South Bengaluru"));
    }

    #[test]
    fn bhk_parser_tolerates_repeated_whitespace() {
        let intent = parse_intent("1  bhk near manipal  hospital within 3 km");

        assert_eq!(intent.bhk, Some(1));
    }

    #[test]
    fn parser_regressions_keep_expected_public_intent_slots() {
        let cases = [
            (
                "1 bhk near manipal hospital within 3 km",
                Some(1),
                None,
                None,
            ),
            ("2 bhk near manipal within 3 km", Some(2), None, None),
            (
                "3bhk near tech park east bangalore",
                Some(3),
                Some("East Bengaluru"),
                None,
            ),
            ("3bhk near tech park", Some(3), None, None),
            (
                "large society near hospital under 2cr",
                None,
                None,
                Some(20_000_000),
            ),
        ];

        for (query, expected_bhk, expected_area, expected_budget) in cases {
            let intent = parse_intent(query);
            assert_eq!(intent.bhk, expected_bhk, "{query}");
            assert_eq!(intent.area.as_deref(), expected_area, "{query}");
            assert_eq!(intent.budget_max, expected_budget, "{query}");
        }
    }

    #[test]
    fn parser_supports_configured_number_words_and_up_to_budget() {
        let intent = parse_intent("three bhk up to 80 lakhs near school");

        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(8_000_000));
        assert!(intent
            .preferences
            .contains(&"social infrastructure".to_string()));
    }

    #[test]
    fn named_localities_do_not_resolve_through_parser_config() {
        let intent = parse_intent("3bhk kadugodi under 2cr");

        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area, None);
    }

    #[test]
    fn test_area_alias_does_not_match_inside_words() {
        let intent = parse_intent("avoid waterlogging and traffic but near tech parks 3bhk");

        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.area, None);
        assert!(intent
            .preferences
            .contains(&"avoid waterlogging risk".to_string()));
        assert!(intent.preferences.contains(&"avoid traffic".to_string()));
        assert!(intent
            .preferences
            .contains(&"social infrastructure".to_string()));
        assert!(!intent.preferences.contains(&"greenery".to_string()));
    }

    #[test]
    fn hospital_query_prioritizes_hospital_social_infra_keys() {
        let intent = parse_intent("peaceful home for parents near hospital");
        let signal = intent
            .positive_preferences
            .iter()
            .find(|signal| signal.raw_text == "social infrastructure")
            .expect("hospital should request social infrastructure");

        assert_eq!(
            &signal.expanded_keys[..2],
            [
                "hospital_access".to_string(),
                "nearby_hospitals".to_string()
            ]
        );
    }

    #[test]
    fn place_specific_social_infra_keeps_umbrella_evidence_keys() {
        let school = parse_intent("young family home near school");
        assert!(has_positive_label(&school, "family friendly"));
        assert!(has_expanded_positive_key(&school, "nearby_schools"));
        assert!(has_expanded_positive_key(&school, "social_infra_score"));

        let academy = parse_intent("home near northstar academy");
        assert!(
            has_expanded_positive_key(&academy, "nearby_schools"),
            "academy should be handled as a generic school-family place type"
        );

        let metro = parse_intent("2bhk near metro for office commute");
        assert!(has_expanded_positive_key(
            &metro,
            "distance_to_nearest_metro_km"
        ));
    }

    #[test]
    fn commute_place_language_maps_to_commute_evidence() {
        let commute = parse_intent("need quick office commute but avoid highway noise");
        assert!(has_positive_label(&commute, "commute"));
        assert!(has_negative_label(&commute, "noise"));

        let traffic = parse_intent("south bangalore apartment but not a daily traffic nightmare");
        assert!(has_negative_label(&traffic, "traffic"));
    }

    #[test]
    fn noisy_main_road_language_maps_to_noise_risk() {
        let intent = parse_intent("family friendly but avoid noisy main road");

        assert!(has_positive_label(&intent, "family friendly"));
        assert!(has_negative_label(&intent, "noise"));
    }

    #[test]
    fn negated_area_is_excluded_not_selected() {
        let intent = parse_intent("near tech parks but quiet not south bangalore 3bhk");

        assert_eq!(intent.area, None);
        assert_eq!(intent.excluded_areas, vec!["South Bengaluru".to_string()]);
        assert_eq!(intent.bhk, Some(3));
        assert!(intent
            .preferences
            .contains(&"quiet neighborhood".to_string()));
        assert!(intent
            .preferences
            .contains(&"social infrastructure".to_string()));
    }

    #[test]
    fn accepted_traffic_tradeoff_is_not_avoid_traffic() {
        let intent =
            parse_intent("I can tolerate traffic if society amenities and clubhouse are excellent");

        assert_eq!(intent.accepted_tradeoffs, vec!["traffic".to_string()]);
        assert!(!intent.preferences.contains(&"avoid traffic".to_string()));
        assert!(intent.preferences.contains(&"amenity quality".to_string()));
    }

    #[test]
    fn unsupported_inventory_requests_are_explicit() {
        let intent = parse_intent("plot or villa style calm layout near Bagalur metro");

        assert_eq!(
            intent.unsupported_inventory_types,
            vec!["plot".to_string(), "villa".to_string()]
        );
        assert!(intent.preferences.contains(&"metro access".to_string()));
        assert!(intent
            .preferences
            .contains(&"quiet neighborhood".to_string()));
    }

    #[test]
    fn test_parse_budget() {
        let intent = parse_intent("under 1.5cr in south bangalore");
        assert_eq!(intent.budget_min, None);
        assert_eq!(intent.budget_max, Some(15_000_000));
        assert_eq!(intent.area.as_deref(), Some("South Bengaluru"));
    }

    #[test]
    fn parses_budget_minimum_without_collapsing_to_max() {
        let intent = parse_intent("Budget above 1.5Cr");
        assert_eq!(intent.budget_min, Some(15_000_000));
        assert_eq!(intent.budget_max, None);
    }

    #[test]
    fn parses_budget_ranges() {
        let dash = parse_intent("1.5–2Cr budget");
        assert_eq!(dash.budget_min, Some(15_000_000));
        assert_eq!(dash.budget_max, Some(20_000_000));

        let between = parse_intent("Between 1.8Cr and 2.2Cr");
        assert_eq!(between.budget_min, Some(18_000_000));
        assert_eq!(between.budget_max, Some(22_000_000));
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
        assert_eq!(intent.budget_min, None);
        assert_eq!(intent.budget_max, Some(8_000_000));
    }

    #[test]
    fn parses_punctuated_and_typo_budget_phrases() {
        let intent = parse_intent("east blr 3bhk undr 2.5cr, gud reviews");

        assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
        assert_eq!(intent.bhk, Some(3));
        assert_eq!(intent.budget_max, Some(25_000_000));
        assert!(has_positive_label(&intent, "review quality"));

        let sentence = parse_intent("Budget below 1.5Cr.");
        assert_eq!(sentence.budget_max, Some(15_000_000));
    }

    #[test]
    fn test_parse_min_land_area_constraint() {
        let intent = parse_intent("3bhk with greenery in east bangalore above 10 acres");
        assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
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
        let intent = parse_intent("3bhk east bangalore 10+ acres");
        assert_eq!(intent.hard_constraints.len(), 1);
        assert_eq!(intent.hard_constraints[0].value, 10.0);
    }

    #[test]
    fn test_plain_acres_without_min_operator_is_not_hard_constraint() {
        let intent = parse_intent("3bhk east bangalore 10 acres");
        assert!(intent.hard_constraints.is_empty());
    }

    #[test]
    fn test_avoid_waterlogging_and_traffic_extracts_both_risks() {
        let intent = parse_intent("3bhk east bangalore avoid waterlogging and traffic");
        let risks: Vec<&str> = intent
            .negative_preferences
            .iter()
            .map(|preference| preference.raw_text.as_str())
            .collect();
        assert!(risks.contains(&"waterlogging risk"), "{risks:?}");
        assert!(risks.contains(&"traffic"), "{risks:?}");
    }

    // --- Day 62: Project status preference extraction tests ---

    #[test]
    fn test_ready_to_move_preference() {
        let intent = parse_intent("ready to move in east bangalore");
        assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
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
        let intent = parse_intent("under construction south bangalore");
        assert_eq!(intent.area.as_deref(), Some("South Bengaluru"));
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
        let intent = parse_intent("new launch 3bhk east bangalore");
        assert!(
            intent.preferences.contains(&"new launch".to_string()),
            "Expected 'new launch' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn home_state_queries_use_schema_backed_preferences() {
        let delivered = parse_intent("delivered society near metro east bangalore");
        assert!(delivered
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == "delivered society"
                && preference.expanded_keys.contains(&"home_state".to_string())));

        let new_property = parse_intent("new property in south bangalore");
        assert!(new_property
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == "new property"
                && preference
                    .expanded_keys
                    .contains(&"home_age_bucket".to_string())));

        let old_society = parse_intent("old society in east bangalore");
        assert!(old_society
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == "established society"
                && preference
                    .expanded_keys
                    .contains(&"home_age_bucket".to_string())));
    }

    #[test]
    fn test_delayed_preference() {
        let intent = parse_intent("delayed projects in south bangalore");
        assert!(
            intent.preferences.contains(&"delayed".to_string()),
            "Expected 'delayed' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_upcoming_preference() {
        let intent = parse_intent("upcoming projects in east bangalore");
        assert!(
            intent.preferences.contains(&"upcoming".to_string()),
            "Expected 'upcoming' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_immediate_possession_maps_to_ready_to_move() {
        let intent = parse_intent("immediate possession south bangalore");
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' from 'immediate possession', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_completed_maps_to_ready_to_move() {
        let intent = parse_intent("completed projects south bangalore");
        assert!(
            intent.preferences.contains(&"ready to move".to_string()),
            "Expected 'ready to move' from 'completed', got: {:?}",
            intent.preferences
        );
    }

    // --- Day 63: Builder preference pattern tests ---

    #[test]
    fn test_reliable_builder_preference() {
        let intent = parse_intent("reliable builder east bangalore");
        assert!(
            intent.preferences.contains(&"reliable builder".to_string()),
            "Expected 'reliable builder' preference, got: {:?}",
            intent.preferences
        );
        assert_eq!(intent.area.as_deref(), Some("East Bengaluru"));
    }

    #[test]
    fn test_safe_builder_maps_to_reliable_builder() {
        let intent = parse_intent("safe builder no possession delay under 2 crore");
        assert!(
            intent.preferences.contains(&"reliable builder".to_string()),
            "Expected 'reliable builder' from 'safe builder', got: {:?}",
            intent.preferences
        );
        assert!(
            intent.preferences.contains(&"on time delivery".to_string())
                || intent.preferences.contains(&"avoid delay risk".to_string()),
            "Expected a delay signal, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_trusted_builder_preference() {
        let intent = parse_intent("trusted builder south bangalore");
        assert!(
            intent.preferences.contains(&"trusted builder".to_string()),
            "Expected 'trusted builder' preference, got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_on_time_delivery_preference() {
        let intent = parse_intent("on time delivery 3bhk east bangalore");
        assert!(
            intent.preferences.contains(&"on time delivery".to_string()),
            "Expected 'on time delivery' preference, got: {:?}",
            intent.preferences
        );
        assert_eq!(intent.bhk, Some(3));
    }

    #[test]
    fn test_good_builder_maps_to_trusted_builder() {
        let intent = parse_intent("good builder south bangalore");
        assert!(
            intent.preferences.contains(&"trusted builder".to_string()),
            "Expected 'trusted builder' from 'good builder', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_no_delays_maps_to_on_time_delivery() {
        let intent = parse_intent("no delays east bangalore");
        assert!(
            intent.preferences.contains(&"on time delivery".to_string()),
            "Expected 'on time delivery' from 'no delays', got: {:?}",
            intent.preferences
        );
    }

    #[test]
    fn test_avoid_waterlogging_is_negative_preference() {
        let intent = parse_intent("3bhk east bangalore avoid waterlogging");
        assert!(
            intent
                .positive_preferences
                .iter()
                .all(|preference| preference.raw_text != "waterlogging risk"),
            "waterlogging should not be parsed as a positive preference: {:?}",
            intent.positive_preferences
        );
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
        let intent = parse_intent("family 3bhk south bangalore less traffic not delayed");
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

    #[test]
    fn project_names_do_not_create_greenery_preferences() {
        let intent = parse_intent("Prestige Park Grove 3bhk");

        assert_eq!(intent.bhk, Some(3));
        assert!(!intent.preferences.contains(&"greenery".to_string()));
    }

    #[test]
    fn review_and_resident_feedback_intents_come_from_the_schema_registry() {
        let google = parse_intent("good google reviews Prestige Park Grove");
        let google_preference = google
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "review quality")
            .expect("Google review intent should be detected");
        assert!(google_preference
            .expanded_keys
            .contains(&"google_rating".to_string()));
        assert!(!google.preferences.contains(&"greenery".to_string()));

        let long_form = parse_intent(
            "Show homes with real review receipts.\nI want Google review strength, resident snippets and community proof.",
        );
        assert!(has_positive_label(&long_form, "review quality"));

        let reddit = parse_intent("resident feedback on reddit Prestige Raintree Park");
        let resident_preference = reddit
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "reddit discussions")
            .expect("resident feedback intent should be detected");
        assert!(resident_preference
            .expanded_keys
            .contains(&"reddit_thread_count".to_string()));
        assert!(!reddit.preferences.contains(&"greenery".to_string()));
    }

    #[test]
    fn legal_and_builder_language_maps_to_structured_preferences() {
        let legal = parse_intent(
            "Need safe paperwork, RERA clarity and clean legal receipts.\nAvoid projects where possession or legal status is unclear.",
        );
        assert!(has_positive_label(&legal, "legal safety"));
        assert!(has_positive_label(&legal, "ready to move"));

        let builder = parse_intent(
            "Prefer an experienced builder with a visible RERA track record and builder project count.",
        );
        assert!(has_expanded_positive_key(
            &builder,
            "rera_builder_projects_count"
        ));
        assert!(has_positive_label(&builder, "legal safety"));
    }

    #[test]
    fn listing_receipt_language_maps_to_listing_evidence() {
        let listing = parse_intent(
            "Need a larger 4BHK or premium family apartment with price proof.\nBudget can stretch, but I want listing source and area details.",
        );

        assert!(has_positive_label(&listing, "premium"));
        assert!(has_positive_label(&listing, "listing evidence"));
        assert!(has_expanded_positive_key(&listing, "listing_price_4bhk"));
        assert!(has_expanded_positive_key(
            &listing,
            "listing_area_sqft_4bhk"
        ));

        let receipts = parse_intent(
            "Prestige Waterford 3BHK. I want an explainable premium option with legal and listing receipts.",
        );
        assert!(has_positive_label(&receipts, "legal safety"));
        assert!(has_positive_label(&receipts, "listing evidence"));
        assert!(has_expanded_positive_key(&receipts, "rera_status"));
        assert!(has_expanded_positive_key(&receipts, "listing_price_3bhk"));
    }

    #[test]
    fn negated_luxury_language_is_not_positive_premium_or_luxury_buyer() {
        let intent = parse_intent("not luxury, just practical family home with receipts");

        assert!(has_negative_label(&intent, "premium"));
        assert!(!has_positive_label(&intent, "premium"));
        assert_ne!(intent.buyer_archetype, Some(BuyerArchetype::LuxuryBuyer));
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
    }

    #[test]
    fn affirmative_premium_language_still_maps_to_premium_luxury_buyer() {
        let intent = parse_intent("premium high end apartment with listing receipts");

        assert!(has_positive_label(&intent, "premium"));
        assert!(!has_negative_label(&intent, "premium"));
        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::LuxuryBuyer));
    }

    #[test]
    fn data_gap_language_carries_configured_gap_keys() {
        let water = parse_intent("avoid water issues, no tanker dependency");
        let water_pref = water
            .negative_preferences
            .iter()
            .find(|preference| preference.raw_text == "water issues")
            .expect("water issues should be detected");
        assert!(water_pref
            .gap_keys
            .contains(&"operating.tanker_dependence".to_string()));
        assert!(water_pref
            .gap_keys
            .contains(&"water_supply_risk".to_string()));

        let approvals = parse_intent(
            "BBMP approval issues are a hard no. Need approval documents and OC-like confidence.",
        );
        let legal = approvals
            .positive_preferences
            .iter()
            .find(|preference| preference.raw_text == "legal safety")
            .expect("legal safety should be detected");
        assert_eq!(
            legal.gap_keys,
            vec![
                "bbmp_approval_status".to_string(),
                "occupancy_certificate_status".to_string()
            ]
        );
    }

    #[test]
    fn legal_risk_query_maps_to_proof_dimensions() {
        let intent = parse_intent(
            "Legal risk is a hard no: complaints, litigation and builder revocations should be checked from RERA.\nShow options with those receipts, not guesses.",
        );

        assert!(has_positive_label(&intent, "legal safety"));
        assert!(!has_positive_label(&intent, "reliable builder"));
        assert!(has_expanded_negative_key(&intent, "rera_complaints"));
        assert!(!has_expanded_positive_key(
            &intent,
            "rera_builder_revocations"
        ));
    }

    #[test]
    fn approval_and_oc_language_maps_to_legal_safety() {
        let intent = parse_intent("3bhk under 2cr but only if approvals and OC look clean");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::RiskAverse));
        assert!(has_positive_label(&intent, "legal safety"));
        assert!(has_expanded_positive_key(
            &intent,
            "occupancy_certificate_status"
        ));
        assert!(has_expanded_positive_key(&intent, "bbmp_approval_status"));
    }

    #[test]
    fn shady_paperwork_language_maps_to_legal_and_seller_risk() {
        let intent =
            parse_intent("first home buyer, please avoid any shady paperwork or unverified seller");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::RiskAverse));
        assert!(has_negative_label(&intent, "legal risk"));
        assert!(has_expanded_negative_key(&intent, "seller_trust"));
    }

    #[test]
    fn builder_complaint_language_maps_to_builder_risk_and_delivery_keys() {
        let complaint = parse_intent("avoid projects with unclear title or builder complaints");
        assert!(has_negative_label(&complaint, "legal risk"));
        assert!(has_negative_label(&complaint, "builder trust"));
        assert!(has_expanded_negative_key(
            &complaint,
            "delivery_track_record"
        ));
        assert!(!has_expanded_negative_key(
            &complaint,
            "rera_builder_revocations"
        ));

        let delivery =
            parse_intent("need legal clarity more than discount, no builder delivery issues");
        assert!(has_positive_label(&delivery, "legal safety"));
        assert!(has_negative_label(&delivery, "builder trust"));
        assert!(has_expanded_negative_key(
            &delivery,
            "delivery_track_record"
        ));
    }

    #[test]
    fn monsoon_drainage_language_maps_to_negative_risks() {
        let intent = parse_intent(
            "Concerned about monsoon flooding, bad drainage and stagnant rainwater near approach roads.",
        );

        assert!(has_negative_label(&intent, "waterlogging risk"));
        assert!(has_negative_label(&intent, "approach road"));
    }

    #[test]
    fn positive_approach_road_language_is_not_a_negative_risk() {
        let intent = parse_intent("good approach road and access");

        assert!(has_positive_label(&intent, "approach road"));
        assert!(!has_negative_label(&intent, "approach road"));
    }

    #[test]
    fn family_and_investment_query_extracts_both_preferences() {
        let intent = parse_intent("good for family AND good investment");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
        assert!(has_positive_label(&intent, "family friendly"));
        assert!(has_positive_label(&intent, "resale potential"));
    }

    #[test]
    fn water_issue_query_is_negative_not_positive_water_supply() {
        let intent = parse_intent("avoid water issues, no tanker dependency");

        assert!(has_negative_label(&intent, "water issues"));
        assert!(!has_positive_label(&intent, "water supply"));
    }

    #[test]
    fn stable_water_language_with_no_tanker_issue_is_positive() {
        let intent = parse_intent("good water supply with cauvery and no tanker issue");

        assert!(has_positive_label(&intent, "water supply"));
        assert!(has_negative_label(&intent, "water issues"));
    }

    #[test]
    fn maintenance_and_shady_builder_query_extracts_negative_risks() {
        let intent = parse_intent("don't want maintenance headaches or shady builder");

        assert!(has_negative_label(&intent, "maintenance"));
        assert!(has_negative_label(&intent, "builder trust"));
        assert!(!has_positive_label(&intent, "maintenance"));
    }

    #[test]
    fn soft_parent_query_extracts_quiet_and_open_space() {
        let intent =
            parse_intent("something calmer for my parents, less chaos, more breathing room");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
        assert!(has_positive_label(&intent, "quiet neighborhood"));
        assert!(has_expanded_positive_key(&intent, "open_space_score"));
        assert!(!has_negative_label(&intent, "density risk"));
    }

    #[test]
    fn stemmed_phrase_matching_keeps_config_from_needing_plural_duplicates() {
        let open_space = parse_intent("need greener open spaces for parents");
        assert!(has_positive_label(&open_space, "greenery"));
        assert!(!has_negative_label(&open_space, "greenery"));

        let water = parse_intent("avoid water issues and maintenance issues");
        assert!(has_negative_label(&water, "water issues"));
        assert!(has_negative_label(&water, "maintenance"));
    }

    #[test]
    fn value_commute_query_extracts_value_buyer_and_commute() {
        let intent = parse_intent("affordable 2BHK for young couple, good commute");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::ValueBuyer));
        assert_eq!(intent.bhk, Some(2));
        assert!(has_positive_label(&intent, "commute"));
        assert!(has_positive_label(&intent, "value for money"));
    }

    #[test]
    fn broad_buyer_language_maps_to_family_safety_and_premium_tradeoff() {
        let intent =
            parse_intent("2bhk for couple planning kid, prefer safe society over fancy clubhouse");

        assert_eq!(intent.buyer_archetype, Some(BuyerArchetype::Family));
        assert!(has_positive_label(&intent, "family friendly"));
        assert!(has_positive_label(&intent, "legal safety"));
        assert!(has_negative_label(&intent, "premium"));
    }

    #[test]
    fn water_reliability_and_tanker_language_maps_to_water_risk() {
        let tanker = parse_intent("dependable water supply, not tanker based");
        assert!(has_negative_label(&tanker, "water issues"));
        assert!(has_expanded_negative_key(
            &tanker,
            "operating.tanker_dependence"
        ));

        let reliability = parse_intent("check borewell or water reliability before shortlist");
        assert!(has_negative_label(&reliability, "water issues"));
        assert!(has_expanded_negative_key(
            &reliability,
            "project.water_supply_mode"
        ));
    }

    #[test]
    fn maintenance_review_and_security_language_maps_to_configured_dimensions() {
        let positive = parse_intent("residents say upkeep is good and clean common areas");
        assert!(has_positive_label(&positive, "maintenance"));
        assert!(has_positive_label(&positive, "review quality"));

        let negative = parse_intent(
            "avoid leaking walls, lift problems, poor facility management and complaints about security",
        );
        assert!(has_negative_label(&negative, "maintenance"));
        assert!(has_negative_label(&negative, "security"));
        assert!(has_expanded_negative_key(
            &negative,
            "facility_management_issues"
        ));
        assert!(has_expanded_negative_key(&negative, "security_complaints"));
    }

    #[test]
    fn archetype_uses_strongest_configured_phrase_not_first_group() {
        let risk = parse_intent("ready to move, low legal risk, hospital nearby for parents");
        assert_eq!(risk.buyer_archetype, Some(BuyerArchetype::RiskAverse));

        let end_user = parse_intent("self use home, daily upkeep more than resale upside");
        assert_eq!(end_user.buyer_archetype, Some(BuyerArchetype::EndUser));
        assert!(has_positive_label(&end_user, "maintenance"));
        assert!(has_positive_label(&end_user, "liveability"));
        assert!(has_negative_label(&end_user, "investment"));
    }

    #[test]
    fn proof_and_layout_avoidance_language_maps_to_negative_dimensions() {
        let proof = parse_intent("hide anything with weak proof");
        assert!(has_positive_label(&proof, "legal safety"));
        assert!(has_negative_label(&proof, "proof gap"));

        let layout = parse_intent("avoid cramped layouts, poor ventilation and west facing heat");
        assert!(has_negative_label(&layout, "density risk"));
        assert!(has_negative_label(&layout, "layout quality"));
        assert!(has_negative_label(&layout, "facing"));
    }

    fn has_positive_label(intent: &SearchIntent, label: &str) -> bool {
        intent
            .positive_preferences
            .iter()
            .any(|preference| preference.raw_text == label)
    }

    fn has_negative_label(intent: &SearchIntent, label: &str) -> bool {
        intent
            .negative_preferences
            .iter()
            .any(|preference| preference.raw_text == label)
    }

    fn has_expanded_positive_key(intent: &SearchIntent, key: &str) -> bool {
        intent
            .positive_preferences
            .iter()
            .any(|preference| preference.expanded_keys.contains(&key.to_string()))
    }

    fn has_expanded_negative_key(intent: &SearchIntent, key: &str) -> bool {
        intent
            .negative_preferences
            .iter()
            .any(|preference| preference.expanded_keys.contains(&key.to_string()))
    }
}
