use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::graph::WalkStep;
use crate::knowledge::FactValue;
use crate::routes::enrichment::society_node_id;
use crate::serving::{
    LoadedServingBundle, ServingEdgeRecord, ServingEntityRecord, ServingFactIndex,
    ServingFactRecord, ServingSearchMetadataRecord,
};

const ENTITY_CONTEXT_JSON: &str = include_str!("../../app/config/dag/entity_context.json");
const DEFAULT_MAX_TOTAL_CANDIDATES: usize = 40;
const DEFAULT_MAX_PARAGRAPH_WORDS: usize = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextFile {
    pub version: u32,
    #[serde(default)]
    pub status: String,
    pub compose: EntityContextComposeConfig,
    pub traversal: EntityContextTraversalConfig,
    #[serde(default)]
    pub categories: Vec<EntityContextCategoryConfig>,
    #[serde(default)]
    pub fact_templates: Vec<EntityContextFactTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextComposeConfig {
    #[serde(default = "default_max_total_candidates")]
    pub max_total_candidates: usize,
    #[serde(default = "default_max_paragraph_words")]
    pub max_paragraph_words: usize,
    #[serde(default)]
    pub slot_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextTraversalConfig {
    #[serde(default)]
    pub max_hops: usize,
    #[serde(default)]
    pub edge_priority: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextCategoryConfig {
    pub id: String,
    pub label: String,
    #[serde(default = "default_category_max_items")]
    pub max_items: usize,
    #[serde(default)]
    pub polarity: Option<String>,
    #[serde(default)]
    pub edge_types: Vec<String>,
    #[serde(default)]
    pub entity_prefixes: Vec<String>,
    #[serde(default)]
    pub fact_keys: Vec<String>,
    #[serde(default)]
    pub terms: Vec<String>,
    #[serde(default)]
    pub source_priority: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextFactTemplate {
    pub fact_key: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextClause {
    pub text: String,
    pub traversal: Vec<String>,
    pub target_entity_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polarity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextCategoryGroup {
    pub id: String,
    pub label: String,
    pub items: Vec<EntityContextClause>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContextResponse {
    pub anchor_entity_id: String,
    pub summary_paragraph: String,
    pub clauses: Vec<EntityContextClause>,
    pub category_groups: Vec<EntityContextCategoryGroup>,
}

#[derive(Debug, Clone)]
struct CandidateClause {
    clause: EntityContextClause,
    source_rank: usize,
    order: usize,
}

pub fn entity_context_config() -> &'static EntityContextFile {
    static CONFIG: OnceLock<EntityContextFile> = OnceLock::new();
    CONFIG.get_or_init(|| {
        serde_json::from_str(ENTITY_CONTEXT_JSON).expect("entity_context.json must be valid")
    })
}

pub fn compose_entity_context(
    anchor_entity_id: &str,
    bundle: &LoadedServingBundle,
) -> Option<EntityContextResponse> {
    let config = entity_context_config();
    let walk_anchor = resolve_walk_anchor(anchor_entity_id, bundle)?;
    let society_name = entity_display_name(bundle, &walk_anchor);
    let candidates = collect_candidates(config, bundle, &walk_anchor);
    let category_groups = select_category_groups(config, candidates);
    let clauses = category_groups
        .iter()
        .flat_map(|group| group.items.iter().cloned())
        .collect::<Vec<_>>();

    if clauses.is_empty() {
        return None;
    }

    let summary_paragraph = summarize_graph_context(
        &society_name,
        &category_groups,
        config.compose.max_paragraph_words,
    );
    Some(EntityContextResponse {
        anchor_entity_id: walk_anchor,
        summary_paragraph,
        clauses,
        category_groups,
    })
}

fn collect_candidates(
    config: &EntityContextFile,
    bundle: &LoadedServingBundle,
    anchor_entity_id: &str,
) -> Vec<CandidateClause> {
    let mut candidates = Vec::new();
    let mut candidate_entities = HashSet::from([anchor_entity_id.to_string()]);
    let allowed_edges = config
        .traversal
        .edge_priority
        .iter()
        .filter(|edge| !(edge.as_str() == "in_society" && anchor_entity_id.starts_with("society:")))
        .map(String::as_str)
        .collect::<Vec<_>>();
    let steps =
        bundle
            .graph_index
            .walk_bfs(anchor_entity_id, &allowed_edges, config.traversal.max_hops);

    for step in &steps {
        candidate_entities.insert(step.to_entity_id.clone());
        if let Some(candidate) = candidate_for_step(config, bundle, step, candidates.len()) {
            candidates.push(candidate);
        }
        if candidates.len() >= config.compose.max_total_candidates {
            return candidates;
        }
    }

    for entity_id in candidate_entities {
        collect_fact_candidates(config, bundle, &entity_id, &mut candidates);
        if candidates.len() >= config.compose.max_total_candidates {
            break;
        }
    }
    candidates.truncate(config.compose.max_total_candidates);
    candidates
}

fn candidate_for_step(
    config: &EntityContextFile,
    bundle: &LoadedServingBundle,
    step: &WalkStep,
    order: usize,
) -> Option<CandidateClause> {
    let target = entity_record(bundle, &step.to_entity_id);
    let name = entity_display_name(bundle, &step.to_entity_id);
    if name.is_empty() || is_generic_place_entity(&step.to_entity_id, &name) {
        return None;
    }
    if is_low_signal_approach_segment(&step.to_entity_id, &name) {
        return None;
    }
    let category = category_for_entity(config, &step.edge_type, &step.to_entity_id, &name, target)?;
    let edge = edge_record(bundle, step);
    let text = location_text_for_category(&category.id, &step.edge_type, &name);
    Some(CandidateClause {
        source_rank: source_rank(category, edge.map(|edge| edge.source_type.as_str())),
        order,
        clause: EntityContextClause {
            text,
            traversal: vec![step.edge_type.clone()],
            target_entity_id: step.to_entity_id.clone(),
            fact_key: None,
            polarity: category.polarity.clone(),
            category_id: Some(category.id.clone()),
            source_type: edge.map(|edge| edge.source_type.clone()),
            confidence: edge.map(|edge| edge.confidence),
        },
    })
}

fn collect_fact_candidates(
    config: &EntityContextFile,
    bundle: &LoadedServingBundle,
    entity_id: &str,
    candidates: &mut Vec<CandidateClause>,
) {
    let Some(rows) = bundle.fact_index.entity(entity_id) else {
        return;
    };

    for fact in &rows.facts {
        let Some(category) = config
            .categories
            .iter()
            .find(|category| category.fact_keys.iter().any(|key| key == &fact.fact_key))
        else {
            continue;
        };
        if is_weak_visual_fact(&fact.fact_key) {
            continue;
        }
        let Some(text) = fact_display_text(config, &bundle.fact_index, entity_id, fact, category)
        else {
            continue;
        };
        candidates.push(CandidateClause {
            source_rank: source_rank(category, Some(&fact.source_type)),
            order: candidates.len(),
            clause: EntityContextClause {
                text,
                traversal: vec!["fact".to_string()],
                target_entity_id: entity_id.to_string(),
                fact_key: Some(fact.fact_key.clone()),
                polarity: category.polarity.clone(),
                category_id: Some(category.id.clone()),
                source_type: Some(fact.source_type.clone()),
                confidence: Some(fact.confidence),
            },
        });
    }
}

fn select_category_groups(
    config: &EntityContextFile,
    candidates: Vec<CandidateClause>,
) -> Vec<EntityContextCategoryGroup> {
    let mut by_category = HashMap::<String, Vec<CandidateClause>>::new();
    for candidate in candidates {
        let Some(category_id) = candidate.clause.category_id.clone() else {
            continue;
        };
        by_category.entry(category_id).or_default().push(candidate);
    }

    let mut groups = Vec::new();
    for category_id in category_order(config) {
        let Some(category) = config
            .categories
            .iter()
            .find(|category| category.id == category_id)
        else {
            continue;
        };
        let mut candidates = by_category.remove(&category.id).unwrap_or_default();
        candidates.sort_by(|left, right| {
            left.source_rank
                .cmp(&right.source_rank)
                .then_with(|| confidence_sort_key(right).cmp(&confidence_sort_key(left)))
                .then_with(|| left.order.cmp(&right.order))
        });
        let mut seen = HashSet::new();
        let items = candidates
            .into_iter()
            .filter_map(|candidate| {
                let key = clause_dedupe_key(&candidate.clause);
                seen.insert(key).then_some(candidate.clause)
            })
            .take(category.max_items)
            .collect::<Vec<_>>();
        if !items.is_empty() {
            groups.push(EntityContextCategoryGroup {
                id: category.id.clone(),
                label: category.label.clone(),
                items,
            });
        }
    }
    groups
}

fn category_order(config: &EntityContextFile) -> Vec<String> {
    if !config.compose.slot_order.is_empty() {
        return config.compose.slot_order.clone();
    }
    config
        .categories
        .iter()
        .map(|category| category.id.clone())
        .collect()
}

fn confidence_sort_key(candidate: &CandidateClause) -> i32 {
    (candidate.clause.confidence.unwrap_or(0.0) * 1000.0).round() as i32
}

fn source_rank(category: &EntityContextCategoryConfig, source_type: Option<&str>) -> usize {
    let Some(source_type) = source_type else {
        return category.source_priority.len();
    };
    category
        .source_priority
        .iter()
        .position(|source| source.eq_ignore_ascii_case(source_type))
        .unwrap_or(category.source_priority.len())
}

fn resolve_walk_anchor(anchor_entity_id: &str, bundle: &LoadedServingBundle) -> Option<String> {
    if anchor_entity_id.starts_with("society:") {
        let in_entities = bundle
            .entities
            .iter()
            .any(|entity| entity.entity_id == anchor_entity_id);
        let has_graph = bundle.edges.iter().any(|edge| {
            edge.from_entity_id == anchor_entity_id || edge.to_entity_id == anchor_entity_id
        });
        let has_facts = bundle.fact_index.entity(anchor_entity_id).is_some();
        if in_entities || has_graph || has_facts {
            return Some(anchor_entity_id.to_string());
        }
        return None;
    }

    if anchor_entity_id.starts_with("property:") {
        let steps = bundle
            .graph_index
            .walk_out(anchor_entity_id, &["in_society"], 1);
        if let Some(step) = steps.first() {
            return Some(step.to_entity_id.clone());
        }
    }

    if bundle
        .entities
        .iter()
        .any(|entity| entity.entity_id == anchor_entity_id)
    {
        return Some(anchor_entity_id.to_string());
    }

    None
}

pub fn society_anchor_for_property_slug(
    property_slug: &str,
    bundle: &LoadedServingBundle,
) -> Option<String> {
    let property_anchor = if property_slug.starts_with("property:") {
        property_slug.to_string()
    } else {
        format!("property:{property_slug}")
    };
    resolve_walk_anchor(&property_anchor, bundle).or_else(|| {
        let society_guess = society_node_id(property_slug.trim_start_matches("discovered-"));
        bundle
            .entities
            .iter()
            .any(|entity| entity.entity_id == society_guess)
            .then_some(society_guess)
    })
}

fn category_for_entity<'a>(
    config: &'a EntityContextFile,
    edge_type: &str,
    entity_id: &str,
    name: &str,
    entity: Option<&ServingEntityRecord>,
) -> Option<&'a EntityContextCategoryConfig> {
    if matches!(edge_type, "served_by_road" | "in_area") {
        return category_by_id(config, "location");
    }

    let haystack = format!(
        "{} {} {}",
        entity_id,
        name,
        entity
            .map(|entity| entity.searchable_text.as_str())
            .unwrap_or("")
    )
    .to_lowercase();
    config.categories.iter().find(|category| {
        category.edge_types.iter().any(|edge| edge == edge_type)
            && (category
                .entity_prefixes
                .iter()
                .any(|prefix| entity_id.starts_with(prefix))
                || category
                    .terms
                    .iter()
                    .any(|term| haystack.contains(&term.to_lowercase())))
    })
}

fn category_by_id<'a>(
    config: &'a EntityContextFile,
    category_id: &str,
) -> Option<&'a EntityContextCategoryConfig> {
    config
        .categories
        .iter()
        .find(|category| category.id == category_id)
}

fn entity_record<'a>(
    bundle: &'a LoadedServingBundle,
    entity_id: &str,
) -> Option<&'a ServingEntityRecord> {
    bundle
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
}

fn edge_record<'a>(
    bundle: &'a LoadedServingBundle,
    step: &WalkStep,
) -> Option<&'a ServingEdgeRecord> {
    bundle.edges.iter().find(|edge| {
        edge.from_entity_id == step.from_entity_id
            && edge.edge_type == step.edge_type
            && edge.to_entity_id == step.to_entity_id
    })
}

fn entity_display_name(bundle: &LoadedServingBundle, entity_id: &str) -> String {
    if let Some(name) = bundle.fact_index.entity(entity_id).and_then(|rows| {
        rows.facts.iter().find_map(|fact| {
            if matches!(
                fact.fact_key.as_str(),
                "rera_project_name" | "society_name" | "name"
            ) {
                fact_value_text(&fact.value)
            } else {
                None
            }
        })
    }) {
        return clean_display_name(&name, entity_id);
    }

    if let Some(name) = bundle
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .map(|entity| entity.name.clone())
        .filter(|name| !name.trim().is_empty())
    {
        return clean_display_name(&name, entity_id);
    }

    title_case_slug(
        entity_id
            .split(':')
            .nth(1)
            .unwrap_or(entity_id)
            .replace('-', " ")
            .as_str(),
    )
}

fn clean_display_name(name: &str, entity_id: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return title_case_slug(
            entity_id
                .split(':')
                .nth(1)
                .unwrap_or(entity_id)
                .replace('-', " ")
                .as_str(),
        );
    }
    let cleaned = trimmed
        .strip_suffix(" approach road")
        .or_else(|| trimmed.strip_prefix("Schools near "))
        .unwrap_or(trimmed);
    title_case_slug(cleaned)
}

fn title_case_slug(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            if is_known_acronym(word) {
                return word.to_string();
            }
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_known_acronym(word: &str) -> bool {
    matches!(word, "ECC" | "STP" | "RERA" | "BBMP" | "ITPL")
}

fn location_text_for_category(category_id: &str, edge_type: &str, name: &str) -> String {
    match (category_id, edge_type) {
        ("location", "served_by_road") => format!("sits on {name}"),
        ("location", "in_area") => format!("in {name}"),
        _ => format!("{name} is nearby"),
    }
}

fn is_generic_place_entity(entity_id: &str, name: &str) -> bool {
    entity_id.ends_with("-nearby-schools") || name.starts_with("Schools near ")
}

fn is_low_signal_approach_segment(entity_id: &str, name: &str) -> bool {
    entity_id.starts_with("road_segment:") && !name.to_lowercase().contains("road")
}

fn is_weak_visual_fact(fact_key: &str) -> bool {
    matches!(
        fact_key,
        "approach_road_visual_available" | "media.approach_road_frames"
    )
}

fn fact_display_text(
    config: &EntityContextFile,
    fact_index: &ServingFactIndex,
    entity_id: &str,
    fact: &ServingFactRecord,
    category: &EntityContextCategoryConfig,
) -> Option<String> {
    if let Some(text) = configured_fact_text(config, fact) {
        return Some(text);
    }

    if let Some(template) = fact_index
        .entity(entity_id)
        .and_then(|rows| {
            rows.search_metadata
                .iter()
                .find(|meta| meta.fact_key == fact.fact_key)
        })
        .and_then(|meta: &ServingSearchMetadataRecord| meta.display_template.clone())
    {
        let value = fact_value_text(&fact.value).unwrap_or_else(|| "mentioned".to_string());
        let rendered = template.replace("{value}", &value);
        if !rendered.trim().is_empty() && !rendered.contains("approach-road") {
            return Some(ensure_sentence(rendered));
        }
    }

    let value = fact_value_text(&fact.value)?;
    match category.id.as_str() {
        "education" => Some(format!("Schools nearby include {value}.")),
        "transit" => Some(format!("Metro access includes {value}.")),
        "healthcare" => Some(format!("Hospitals nearby include {value}.")),
        "work" => Some(format!("Work hubs nearby include {value}.")),
        "daily_needs" => Some(format!("Daily needs nearby include {value}.")),
        "reviews" => Some(ensure_sentence(value)),
        "cautions" => Some(ensure_sentence(value)),
        _ => Some(ensure_sentence(value)),
    }
}

fn configured_fact_text(config: &EntityContextFile, fact: &ServingFactRecord) -> Option<String> {
    let template = config
        .fact_templates
        .iter()
        .find(|template| template.fact_key == fact.fact_key)?;
    let rendered = if template.template.contains("{value}") {
        let value = fact_value_text(&fact.value)?;
        template.template.replace("{value}", &value)
    } else {
        template.template.clone()
    };
    Some(ensure_sentence(rendered))
}

fn fact_value_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::Text(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        FactValue::Bool(flag) => Some(flag.to_string()),
        FactValue::Numeric(number) if number.is_finite() => Some(trim_numeric(number.to_string())),
        FactValue::Tags(tags) if !tags.is_empty() => Some(tags.join(", ")),
        _ => None,
    }
}

fn trim_numeric(mut text: String) -> String {
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

fn summarize_graph_context(
    society_name: &str,
    category_groups: &[EntityContextCategoryGroup],
    max_words: usize,
) -> String {
    let mut sentences = Vec::new();
    let roads = clause_subjects(category_groups, "location", |clause| {
        clause.text.strip_prefix("sits on ").map(str::to_string)
    });
    let areas = clause_subjects(category_groups, "location", |clause| {
        clause.text.strip_prefix("in ").map(str::to_string)
    });

    match (roads.first(), areas.first()) {
        (Some(road), Some(area)) => {
            sentences.push(format!("{society_name} sits on {road} in {area}."));
        }
        (Some(road), None) => {
            sentences.push(format!("{society_name} sits on {road}."));
        }
        (None, Some(area)) => {
            sentences.push(format!("{society_name} is in {area}."));
        }
        (None, None) => {}
    }

    let nearby = [
        clause_names(category_groups, "education"),
        clause_names(category_groups, "healthcare"),
        clause_names(category_groups, "daily_needs"),
    ]
    .concat();
    if !nearby.is_empty() {
        sentences.push(format!("Nearby are {}.", join_natural_list(&nearby)));
    }

    let commute = [
        clause_names(category_groups, "transit"),
        clause_names(category_groups, "work"),
    ]
    .concat();
    if !commute.is_empty() {
        sentences.push(format!(
            "For commute, {} are nearby.",
            join_natural_list(&commute)
        ));
    }

    append_group_sentences(category_groups, "reviews", &mut sentences);
    append_group_sentences(category_groups, "cautions", &mut sentences);

    if sentences.is_empty() {
        for group in category_groups {
            for item in &group.items {
                sentences.push(ensure_sentence(item.text.clone()));
            }
        }
    }

    truncate_words(&sentences.join(" "), max_words)
}

fn append_group_sentences(
    category_groups: &[EntityContextCategoryGroup],
    category_id: &str,
    sentences: &mut Vec<String>,
) {
    let Some(group) = category_groups.iter().find(|group| group.id == category_id) else {
        return;
    };
    for item in &group.items {
        let sentence = ensure_sentence(item.text.clone());
        if !sentences.iter().any(|existing| existing == &sentence) {
            sentences.push(sentence);
        }
    }
}

fn clause_names(category_groups: &[EntityContextCategoryGroup], category_id: &str) -> Vec<String> {
    clause_subjects(category_groups, category_id, |clause| {
        clause
            .text
            .strip_suffix(" is nearby")
            .map(str::to_string)
            .or_else(|| {
                clause
                    .text
                    .split_once(" include ")
                    .map(|(_, value)| value.trim_end_matches('.').to_string())
            })
    })
}

fn clause_subjects<F>(
    category_groups: &[EntityContextCategoryGroup],
    category_id: &str,
    extractor: F,
) -> Vec<String>
where
    F: Fn(&EntityContextClause) -> Option<String>,
{
    let Some(group) = category_groups.iter().find(|group| group.id == category_id) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    group
        .items
        .iter()
        .filter_map(extractor)
        .filter(|name| !name.trim().is_empty())
        .filter(|name| seen.insert(normalized_key(name)))
        .collect()
}

fn clause_dedupe_key(clause: &EntityContextClause) -> String {
    if clause.category_id.as_deref() == Some("location") {
        if let Some(name) = clause
            .text
            .strip_prefix("sits on ")
            .or_else(|| clause.text.strip_prefix("in "))
        {
            return format!(
                "{}:{}",
                clause.category_id.as_deref().unwrap_or(""),
                normalized_key(name)
            );
        }
    }
    if let Some(name) = clause.text.strip_suffix(" is nearby") {
        return format!(
            "{}:{}",
            clause.category_id.as_deref().unwrap_or(""),
            normalized_key(name)
        );
    }
    format!(
        "{}:{}:{}",
        clause.category_id.as_deref().unwrap_or(""),
        clause.fact_key.as_deref().unwrap_or(""),
        normalized_key(&clause.text)
    )
}

fn normalized_key(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn ensure_sentence(text: String) -> String {
    let trimmed = text.trim();
    if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

fn join_natural_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let (head, tail) = items.split_at(items.len() - 1);
            format!("{}, and {}", head.join(", "), tail[0])
        }
    }
}

fn truncate_words(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        return text.to_string();
    }
    format!("{}...", words[..max_words].join(" "))
}

fn default_max_total_candidates() -> usize {
    DEFAULT_MAX_TOTAL_CANDIDATES
}

fn default_max_paragraph_words() -> usize {
    DEFAULT_MAX_PARAGRAPH_WORDS
}

fn default_category_max_items() -> usize {
    3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphIndex;
    use crate::serving::{
        ServingBundleManifest, ServingEdgeRecord, ServingEntityRecord, ServingFactIndex,
        ServingFactRecord, TantivyRecallIndex,
    };
    use chrono::Utc;
    use tempfile::tempdir;

    fn serving_fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        confidence: f32,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "text".to_string(),
            value_text: fact_value_text(&value),
            value,
            confidence,
            source_type: "Google".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at: Utc::now(),
        }
    }

    fn entity(entity_id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            searchable_text: format!("{entity_id} {entity_type} {name}"),
        }
    }

    fn edge(from: &str, edge_type: &str, to: &str) -> ServingEdgeRecord {
        ServingEdgeRecord {
            from_entity_id: from.to_string(),
            edge_type: edge_type.to_string(),
            to_entity_id: to.to_string(),
            confidence: 0.9,
            source_type: "LocalContextSeed".to_string(),
        }
    }

    // Golden regression fixture for the summarizer. Keep this small and replace it
    // with a bundle-backed fixture once context quality fixtures are promoted.
    fn waterford_fixture_bundle() -> LoadedServingBundle {
        let edges = vec![
            edge(
                "society:prestige-waterford",
                "served_by_road",
                "road:ecc-road",
            ),
            edge(
                "society:prestige-waterford",
                "served_by_road",
                "road_segment:prestige-waterford-approach",
            ),
            edge(
                "society:prestige-waterford",
                "in_area",
                "area:pattandur-agrahara",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:deens-public-school",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:hopefarm-channasandra-metro",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:seegehalli-metro",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:prestige-shantiniketan-mall",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:manipal-hospital-whitefield",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:whitefield-marriott",
            ),
            edge(
                "society:prestige-waterford",
                "near_place",
                "place:bagmane-tech-park",
            ),
            edge(
                "society:prestige-waterford",
                "maps_to_place",
                "place:prestige-waterford-nearby-schools",
            ),
        ];
        let entities = vec![
            entity(
                "society:prestige-waterford",
                "society",
                "prestige waterford",
            ),
            entity("road:ecc-road", "road_segment", "ECC Road"),
            entity(
                "road_segment:prestige-waterford-approach",
                "road_segment",
                "Prestige Waterford approach road",
            ),
            entity("area:pattandur-agrahara", "area", "Pattandur Agrahara"),
            entity("place:deens-public-school", "place", "Deens Public School"),
            entity(
                "place:hopefarm-channasandra-metro",
                "place",
                "Hopefarm Channasandra metro",
            ),
            entity("place:seegehalli-metro", "place", "Seegehalli metro"),
            entity(
                "place:prestige-shantiniketan-mall",
                "place",
                "Prestige Shantiniketan mall",
            ),
            entity(
                "place:manipal-hospital-whitefield",
                "place",
                "Manipal Hospital Whitefield",
            ),
            entity("place:whitefield-marriott", "place", "Whitefield Marriott"),
            entity("place:bagmane-tech-park", "place", "Bagmane Tech Park"),
            entity(
                "place:prestige-waterford-nearby-schools",
                "place",
                "Schools near Prestige Waterford",
            ),
        ];
        let facts = vec![
            serving_fact(
                "society:prestige-waterford",
                "rera_project_name",
                FactValue::Text("PRESTIGE WATERFORD".to_string()),
                0.95,
            ),
            serving_fact(
                "society:prestige-waterford",
                "google_rating",
                FactValue::Numeric(4.4),
                0.8,
            ),
            serving_fact(
                "society:prestige-waterford",
                "community_positive_themes",
                FactValue::Tags(vec![
                    "amenities".to_string(),
                    "greenery".to_string(),
                    "connectivity".to_string(),
                ]),
                0.6,
            ),
            serving_fact(
                "society:prestige-waterford",
                "community_concern_themes",
                FactValue::Tags(vec!["traffic".to_string()]),
                0.6,
            ),
            serving_fact(
                "road:ecc-road",
                "risk.approach_road_waterlogging",
                FactValue::Text("mentioned".to_string()),
                0.5,
            ),
        ];
        let fact_index = ServingFactIndex::from_records(facts, Vec::new());
        let cache_dir = tempdir().expect("tempdir");
        let recall_index =
            TantivyRecallIndex::build_in_dir(cache_dir.path(), &entities, &[], &[]).expect("index");

        LoadedServingBundle {
            manifest: ServingBundleManifest {
                bundle_version: "test".to_string(),
                format_version: 3,
                created_at: Utc::now(),
                entity_count: entities.len() as u64,
                fact_count: fact_index.all_facts().len() as u64,
                search_metadata_count: 0,
                edge_count: edges.len() as u64,
                entity_parquet_key: "entities".to_string(),
                fact_parquet_key: "facts".to_string(),
                search_metadata_parquet_key: "search_metadata".to_string(),
                edge_parquet_key: Some("edges".to_string()),
                schema_key: "schema".to_string(),
                trust_policy_key: "trust".to_string(),
                tantivy_index_prefix: "tantivy".to_string(),
                artifacts: Vec::new(),
            },
            entities,
            edges: edges.clone(),
            graph_index: GraphIndex::from_serving_edges(&edges),
            recall_index,
            fact_index,
            cache_dir: cache_dir.path().to_path_buf(),
        }
    }

    #[test]
    fn entity_context_config_loads_category_budgets() {
        let config = entity_context_config();
        assert!(config.compose.max_total_candidates > 0);
        assert!(config
            .categories
            .iter()
            .any(|category| category.id == "transit"));
        assert!(config
            .categories
            .iter()
            .any(|category| category.id == "cautions"));
    }

    #[test]
    fn compose_entity_context_builds_high_quality_waterford_paragraph() {
        let bundle = waterford_fixture_bundle();
        let context =
            compose_entity_context("society:prestige-waterford", &bundle).expect("context");
        let summary = &context.summary_paragraph;
        assert!(summary.contains("Prestige Waterford sits on ECC Road"));
        assert!(summary.contains("Pattandur Agrahara"));
        assert!(summary.contains("Deens Public School"));
        assert!(summary.contains("Hopefarm Channasandra Metro"));
        assert!(summary.contains("Seegehalli Metro"));
        assert!(summary.contains("Prestige Shantiniketan Mall"));
        assert!(summary.contains("Manipal Hospital Whitefield"));
        assert!(summary.contains("Bagmane Tech Park"));
        assert!(summary.contains("waterlogging"));
        assert!(!summary.contains("risk.approach"));
        assert!(!summary.contains("nearby-schools"));
        assert!(!summary.contains("approach-road"));
        assert!(
            summary.split_whitespace().count()
                <= entity_context_config().compose.max_paragraph_words
        );
        assert!(context
            .category_groups
            .iter()
            .any(|group| group.id == "transit"));
        assert!(context
            .category_groups
            .iter()
            .any(|group| group.id == "work"));
    }

    #[test]
    fn weak_evidence_society_gets_short_factual_fallback() {
        let edges = vec![edge("society:quiet-home", "in_area", "area:whitefield")];
        let entities = vec![
            entity("society:quiet-home", "society", "Quiet Home"),
            entity("area:whitefield", "area", "Whitefield"),
        ];
        let fact_index = ServingFactIndex::from_records(Vec::new(), Vec::new());
        let cache_dir = tempdir().expect("tempdir");
        let recall_index =
            TantivyRecallIndex::build_in_dir(cache_dir.path(), &entities, &[], &[]).expect("index");
        let bundle = LoadedServingBundle {
            manifest: ServingBundleManifest {
                bundle_version: "test".to_string(),
                format_version: 3,
                created_at: Utc::now(),
                entity_count: entities.len() as u64,
                fact_count: 0,
                search_metadata_count: 0,
                edge_count: edges.len() as u64,
                entity_parquet_key: "entities".to_string(),
                fact_parquet_key: "facts".to_string(),
                search_metadata_parquet_key: "search_metadata".to_string(),
                edge_parquet_key: Some("edges".to_string()),
                schema_key: "schema".to_string(),
                trust_policy_key: "trust".to_string(),
                tantivy_index_prefix: "tantivy".to_string(),
                artifacts: Vec::new(),
            },
            entities,
            edges: edges.clone(),
            graph_index: GraphIndex::from_serving_edges(&edges),
            recall_index,
            fact_index,
            cache_dir: cache_dir.path().to_path_buf(),
        };
        let context = compose_entity_context("society:quiet-home", &bundle).expect("context");
        assert_eq!(context.summary_paragraph, "Quiet Home is in Whitefield.");
        assert!(!context.summary_paragraph.contains("unknown"));
    }
}
