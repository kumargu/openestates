use serde::Serialize;

use crate::dag_config::{search_guardrail_config, SearchGuidanceTemplate};

use super::intent::{self, SearchIntent};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchGuidance {
    pub mode: String,
    pub title: String,
    pub message: String,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GuardedSearch {
    pub intent: SearchIntent,
    pub guidance: SearchGuidance,
}

pub fn no_results_guidance() -> SearchGuidance {
    guidance_from_template(&search_guardrail_config().guidance.no_results, None)
}

pub fn guard_search_query(query: &str) -> Option<GuardedSearch> {
    let config = search_guardrail_config();
    let normalized = normalize_query(query);
    if normalized.is_empty() {
        return Some(guarded(query, &config.guidance.empty_query));
    }

    let intent = intent::parse_intent(query);
    let tokens = tokens(&normalized);
    let structured_score = structured_home_intent_score(&intent);
    let home_intent_score = structured_score + lexical_home_intent_score(&normalized);

    if is_assistant_directed_question(&normalized, structured_score) {
        return Some(GuardedSearch {
            intent,
            guidance: guidance_from_template(&config.guidance.out_of_scope, None),
        });
    }

    if !intent.unsupported_inventory_types.is_empty() {
        let inventory = intent.unsupported_inventory_types.join(", ");
        return Some(GuardedSearch {
            intent,
            guidance: guidance_from_template(
                &config.guidance.unsupported_inventory,
                Some(&inventory),
            ),
        });
    }

    if tokens.len() < config.too_short.min_tokens
        && !has_structured_anchor(&intent)
        && home_intent_score < config.home_intent_detection.minimum_short_query_score
    {
        return Some(guarded(query, &config.guidance.too_short));
    }

    if query_contains_any_pattern(&normalized, &config.decision_brief.patterns) {
        return Some(guarded(query, &config.guidance.decision_brief));
    }

    if is_vague_home_query(&normalized, &intent) {
        return Some(GuardedSearch {
            intent,
            guidance: guidance_from_template(&config.guidance.needs_more_specifics, None),
        });
    }

    if is_weak_anchor_only(&normalized, &tokens, &intent) {
        return Some(GuardedSearch {
            intent,
            guidance: guidance_from_template(&config.guidance.needs_home_anchor, None),
        });
    }

    if home_intent_score < config.home_intent_detection.minimum_positive_score {
        return Some(GuardedSearch {
            intent,
            guidance: guidance_from_template(&config.guidance.out_of_scope, None),
        });
    }

    None
}

fn guarded(query: &str, template: &SearchGuidanceTemplate) -> GuardedSearch {
    GuardedSearch {
        intent: intent::parse_intent(query),
        guidance: guidance_from_template(template, None),
    }
}

fn guidance_from_template(
    template: &SearchGuidanceTemplate,
    inventory: Option<&str>,
) -> SearchGuidance {
    let mut message = template.message.clone();
    if let Some(inventory) = inventory {
        message = message.replace("{inventory}", inventory);
    }
    SearchGuidance {
        mode: template.mode.clone(),
        title: template.title.clone(),
        message,
        suggestions: template.suggestions.clone(),
    }
}

fn normalize_query(query: &str) -> String {
    query
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokens(query: &str) -> Vec<&str> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect()
}

fn is_vague_home_query(query: &str, intent: &SearchIntent) -> bool {
    let config = search_guardrail_config();
    query_contains_any_pattern(query, &config.vague_home_query.patterns)
        && !has_structured_anchor(intent)
}

fn is_weak_anchor_only(query: &str, tokens: &[&str], intent: &SearchIntent) -> bool {
    let config = search_guardrail_config();
    if has_structured_anchor(intent) || has_strong_lexical_home_anchor(query) {
        return false;
    }

    tokens.len() <= config.home_intent_detection.weak_anchor_max_tokens
        && config
            .home_intent_detection
            .weak_anchor_terms
            .iter()
            .any(|term| query_contains_term(query, term))
}

fn structured_home_intent_score(intent: &SearchIntent) -> i32 {
    let scores = &search_guardrail_config()
        .home_intent_detection
        .structured_signal_scores;
    let mut score = 0;
    if intent.area.is_some() {
        score += scores.area;
    }
    if intent.bhk.is_some() {
        score += scores.bhk;
    }
    if intent.budget_max.is_some() {
        score += scores.budget_max;
    }
    if !intent.excluded_areas.is_empty() {
        score += scores.excluded_area;
    }
    if !intent.hard_constraints.is_empty() {
        score += scores.hard_constraint;
    }
    if !intent.positive_preferences.is_empty() || !intent.negative_preferences.is_empty() {
        score += scores.preference;
    }
    if intent.buyer_archetype.is_some() {
        score += scores.buyer_archetype;
    }
    if !intent.unsupported_inventory_types.is_empty() {
        score += scores.unsupported_inventory;
    }
    score
}

fn lexical_home_intent_score(query: &str) -> i32 {
    search_guardrail_config()
        .home_intent_detection
        .term_groups
        .iter()
        .filter(|group| {
            group
                .terms
                .iter()
                .any(|term| query_contains_term(query, term))
                || group
                    .substrings
                    .iter()
                    .any(|substring| query.contains(&substring.to_ascii_lowercase()))
        })
        .map(|group| group.score)
        .sum()
}

fn has_strong_lexical_home_anchor(query: &str) -> bool {
    let config = search_guardrail_config();
    config
        .home_intent_detection
        .term_groups
        .iter()
        .any(|group| {
            group.score >= config.home_intent_detection.minimum_positive_score
                && (group
                    .terms
                    .iter()
                    .any(|term| query_contains_term(query, term))
                    || group
                        .substrings
                        .iter()
                        .any(|substring| query.contains(&substring.to_ascii_lowercase())))
        })
}

fn has_structured_anchor(intent: &SearchIntent) -> bool {
    intent.area.is_some()
        || intent.bhk.is_some()
        || intent.budget_max.is_some()
        || !intent.excluded_areas.is_empty()
        || !intent.hard_constraints.is_empty()
        || !intent.positive_preferences.is_empty()
        || !intent.negative_preferences.is_empty()
}

fn is_assistant_directed_question(query: &str, structured_score: i32) -> bool {
    let config = search_guardrail_config();
    if structured_score > config.assistant_directed_question.max_structured_score {
        return false;
    }

    let Some(first_token) = tokens(query).first().copied() else {
        return false;
    };

    let starts_like_question = config
        .assistant_directed_question
        .question_starters
        .iter()
        .any(|term| first_token == term.as_str());
    let addresses_assistant = config
        .assistant_directed_question
        .assistant_subject_terms
        .iter()
        .any(|term| query_contains_token(query, term));
    let asks_for_search = config
        .assistant_directed_question
        .allowed_search_action_terms
        .iter()
        .any(|term| query_contains_term(query, term));

    starts_like_question && addresses_assistant && !asks_for_search
}

fn query_contains_any_pattern(query: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| query.contains(&pattern.to_ascii_lowercase()))
}

fn query_contains_term(query: &str, term: &str) -> bool {
    let normalized_term = term.to_ascii_lowercase();
    if normalized_term.contains(' ') {
        query.contains(&normalized_term)
    } else {
        query_contains_token(query, &normalized_term)
    }
}

fn query_contains_token(query: &str, token: &str) -> bool {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_without_home_intent_is_guarded() {
        let guarded = guard_search_query("what should I do with my life").unwrap();

        assert_eq!(guarded.guidance.mode, "out_of_scope");
        assert!(guarded.guidance.title.contains("home"));
    }

    #[test]
    fn meta_home_prompt_is_guarded() {
        let guarded = guard_search_query("are you looking for a home").unwrap();

        assert_eq!(guarded.guidance.mode, "out_of_scope");
    }

    #[test]
    fn assistant_style_structured_buyer_query_passes_through() {
        assert!(guard_search_query("can you help me choose 3bhk under 2cr").is_none());
    }

    #[test]
    fn vague_home_query_asks_for_specifics() {
        let guarded = guard_search_query("find me something good").unwrap();

        assert_eq!(guarded.guidance.mode, "needs_more_specifics");
    }

    #[test]
    fn single_token_structured_home_query_passes_through() {
        assert!(guard_search_query("3bhk").is_none());
    }

    #[test]
    fn unsupported_inventory_is_guarded() {
        let guarded = guard_search_query("villa or plot style calm layout near metro").unwrap();

        assert_eq!(guarded.guidance.mode, "unsupported_inventory");
        assert_eq!(
            guarded.intent.unsupported_inventory_types,
            vec!["plot".to_string(), "villa".to_string()]
        );
    }

    #[test]
    fn valid_buyer_query_passes_through() {
        assert!(guard_search_query("3bhk whitefield under 2cr avoid water issues").is_none());
    }

    #[test]
    fn area_only_non_home_task_is_guarded() {
        let guarded = guard_search_query("weather in Whitefield").unwrap();

        assert_eq!(guarded.guidance.mode, "out_of_scope");
    }
}
