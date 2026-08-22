use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::dag_config::{area_alias_entries, search_resolution_config};
use crate::search::schema;

use super::{ServingEdgeRecord, ServingEntityRecord};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServingEntityAliasRecord {
    pub alias: String,
    pub normalized_alias: String,
    pub entity_id: String,
    pub entity_type: String,
    pub entity_name: String,
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct ServingEntityAliasIndex {
    by_alias: HashMap<String, ServingEntityAliasGroup>,
    max_token_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServingEntityAliasGroup {
    pub alias: String,
    pub normalized_alias: String,
    pub members: Vec<ServingEntityAliasRecord>,
}

impl ServingEntityAliasIndex {
    pub fn from_records(
        records: Vec<ServingEntityAliasRecord>,
    ) -> Result<Self, ServingEntityAliasError> {
        validate_alias_groups(&records)?;
        let mut records_by_alias = BTreeMap::<String, Vec<ServingEntityAliasRecord>>::new();
        let mut max_token_count = 0;
        for record in records {
            let normalized = normalize_alias(&record.alias);
            if normalized.is_empty() || normalized != record.normalized_alias {
                return Err(ServingEntityAliasError::Invalid(format!(
                    "alias {:?} has invalid normalized value {:?}",
                    record.alias, record.normalized_alias
                )));
            }
            max_token_count = max_token_count.max(normalized.split_whitespace().count());
            records_by_alias.entry(normalized).or_default().push(record);
        }
        let by_alias = records_by_alias
            .into_iter()
            .map(|(normalized_alias, members)| {
                let alias = members[0].alias.clone();
                (
                    normalized_alias.clone(),
                    ServingEntityAliasGroup {
                        alias,
                        normalized_alias,
                        members,
                    },
                )
            })
            .collect();
        Ok(Self {
            by_alias,
            max_token_count,
        })
    }

    pub fn get(&self, alias: &str) -> Option<&ServingEntityAliasGroup> {
        self.by_alias.get(&normalize_alias(alias))
    }

    pub fn max_token_count(&self) -> usize {
        self.max_token_count
    }

    pub fn records(&self) -> impl Iterator<Item = &ServingEntityAliasRecord> {
        self.by_alias
            .values()
            .flat_map(|group| group.members.iter())
    }

    pub fn len(&self) -> usize {
        self.by_alias
            .values()
            .map(|group| group.members.len())
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_alias.is_empty()
    }
}

pub fn materialize_society_aliases(
    entities: &[ServingEntityRecord],
    edges: &[ServingEdgeRecord],
) -> Result<Vec<ServingEntityAliasRecord>, ServingEntityAliasError> {
    let entity_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity))
        .collect::<HashMap<_, _>>();
    let protected = AliasCollisionVocabulary::new(entities);
    let min_chars = search_resolution_config().min_partial_entity_name_chars;
    let mut candidates = Vec::new();
    for edge in edges.iter().filter(|edge| edge.edge_type == "built_by") {
        let Some(society) = entity_by_id.get(edge.from_entity_id.as_str()) else {
            continue;
        };
        let Some(builder) = entity_by_id.get(edge.to_entity_id.as_str()) else {
            continue;
        };
        if !society.entity_type.eq_ignore_ascii_case("society")
            || !builder.entity_type.eq_ignore_ascii_case("builder")
        {
            continue;
        }
        let Some(builder_brand) = first_word(&builder.name) else {
            continue;
        };
        for (alias, source) in aliases_from_builder_relation(&society.name, builder_brand) {
            let normalized_alias = normalize_alias(&alias);
            if normalized_alias
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .count()
                < min_chars
                || protected.collides(&normalized_alias)
            {
                continue;
            }
            candidates.push(ServingEntityAliasRecord {
                alias,
                normalized_alias,
                entity_id: society.entity_id.clone(),
                entity_type: society.entity_type.clone(),
                entity_name: society.name.clone(),
                source: source.to_string(),
            });
        }
    }

    let mut candidates_by_alias = BTreeMap::<String, Vec<ServingEntityAliasRecord>>::new();
    for candidate in &candidates {
        candidates_by_alias
            .entry(candidate.normalized_alias.clone())
            .or_default()
            .push(candidate.clone());
    }
    let accepted_aliases = candidates_by_alias
        .iter()
        .filter_map(|(normalized_alias, group)| {
            let group = group.iter().collect::<Vec<_>>();
            alias_group_is_unique_or_related_phase_family(&group)
                .then_some(normalized_alias.as_str())
        })
        .collect::<HashSet<_>>();
    candidates.retain(|candidate| accepted_aliases.contains(candidate.normalized_alias.as_str()));
    candidates.sort_by(|left, right| {
        left.normalized_alias
            .cmp(&right.normalized_alias)
            .then_with(|| left.entity_id.cmp(&right.entity_id))
            .then_with(|| left.source.cmp(&right.source))
    });
    candidates.dedup_by(|left, right| {
        left.normalized_alias == right.normalized_alias && left.entity_id == right.entity_id
    });
    validate_society_aliases(&candidates, entities)?;
    Ok(candidates)
}

pub fn validate_society_aliases(
    aliases: &[ServingEntityAliasRecord],
    entities: &[ServingEntityRecord],
) -> Result<(), ServingEntityAliasError> {
    let entity_by_id = entities
        .iter()
        .map(|entity| (entity.entity_id.as_str(), entity))
        .collect::<HashMap<_, _>>();
    let protected = AliasCollisionVocabulary::new(entities);
    for alias in aliases {
        let normalized = normalize_alias(&alias.alias);
        if normalized.is_empty() || normalized != alias.normalized_alias {
            return Err(ServingEntityAliasError::Invalid(format!(
                "alias {:?} has invalid normalized value {:?}",
                alias.alias, alias.normalized_alias
            )));
        }
        let Some(entity) = entity_by_id.get(alias.entity_id.as_str()) else {
            return Err(ServingEntityAliasError::Invalid(format!(
                "alias {:?} references missing entity {}",
                alias.alias, alias.entity_id
            )));
        };
        if !entity.entity_type.eq_ignore_ascii_case("society")
            || !alias.entity_type.eq_ignore_ascii_case("society")
            || alias.entity_name != entity.name
        {
            return Err(ServingEntityAliasError::Invalid(format!(
                "alias {:?} must reference the canonical society name",
                alias.alias
            )));
        }
        if protected.collides(&normalized) {
            return Err(ServingEntityAliasError::Invalid(format!(
                "alias {:?} collides with entity or generic search vocabulary",
                alias.alias
            )));
        }
    }
    validate_alias_groups(aliases)?;
    Ok(())
}

fn validate_alias_groups(
    aliases: &[ServingEntityAliasRecord],
) -> Result<(), ServingEntityAliasError> {
    let mut aliases_by_normalized = BTreeMap::<&str, Vec<&ServingEntityAliasRecord>>::new();
    for alias in aliases {
        aliases_by_normalized
            .entry(&alias.normalized_alias)
            .or_default()
            .push(alias);
    }
    for (normalized_alias, group) in aliases_by_normalized {
        if !alias_group_is_unique_or_related_phase_family(&group) {
            return Err(ServingEntityAliasError::Invalid(format!(
                "alias {normalized_alias:?} maps to unrelated society entities"
            )));
        }
    }
    Ok(())
}

fn alias_group_is_unique_or_related_phase_family(group: &[&ServingEntityAliasRecord]) -> bool {
    let entity_ids = group
        .iter()
        .map(|record| record.entity_id.as_str())
        .collect::<BTreeSet<_>>();
    if entity_ids.len() <= 1 {
        return true;
    }

    let memberships = group
        .iter()
        .map(|record| phase_family_membership(record))
        .collect::<Option<Vec<_>>>();
    let Some(memberships) = memberships else {
        return false;
    };
    let family_ids = memberships
        .iter()
        .map(|membership| membership.family_id.as_str())
        .collect::<BTreeSet<_>>();
    let phase_ids = memberships
        .iter()
        .map(|membership| membership.phase_id.as_str())
        .collect::<BTreeSet<_>>();
    family_ids.len() == 1 && phase_ids.len() == entity_ids.len()
}

struct PhaseFamilyMembership {
    family_id: String,
    phase_id: String,
}

fn phase_family_membership(record: &ServingEntityAliasRecord) -> Option<PhaseFamilyMembership> {
    if record.source != "builder_byline" {
        return None;
    }
    let normalized_name = normalize_alias(&record.entity_name);
    let name_tokens = normalized_name.split_whitespace().collect::<Vec<_>>();
    let alias_tokens = record
        .normalized_alias
        .split_whitespace()
        .collect::<Vec<_>>();
    if alias_tokens.is_empty()
        || !name_tokens.starts_with(&alias_tokens)
        || name_tokens.get(alias_tokens.len()) != Some(&"by")
    {
        return None;
    }

    let builder_start = alias_tokens.len() + 1;
    let (phase_start, phase_id) = name_tokens
        .iter()
        .enumerate()
        .skip(builder_start + 1)
        .find_map(|(index, token)| {
            if *token == "phase" {
                let phase_id = *name_tokens.get(index + 1)?;
                is_phase_designator(phase_id).then_some((index, phase_id))
            } else {
                let phase_id = token.strip_prefix("phase")?;
                is_phase_designator(phase_id).then_some((index, phase_id))
            }
        })?;
    let builder = name_tokens[builder_start..phase_start].join(" ");
    if builder.is_empty() {
        return None;
    }
    Some(PhaseFamilyMembership {
        family_id: format!("{} by {builder}", record.normalized_alias),
        phase_id: phase_id.to_string(),
    })
}

fn is_phase_designator(value: &str) -> bool {
    value.parse::<u16>().is_ok()
        || matches!(
            value,
            "i" | "ii" | "iii" | "iv" | "v" | "vi" | "vii" | "viii" | "ix" | "x"
        )
}

pub fn normalize_alias(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn aliases_from_builder_relation<'a>(
    society_name: &'a str,
    builder_brand: &str,
) -> Vec<(String, &'static str)> {
    let words = word_spans(society_name);
    let Some(first) = words.first() else {
        return Vec::new();
    };
    let mut aliases = Vec::new();
    if first.text.eq_ignore_ascii_case(builder_brand) {
        let alias = society_name[first.end..].trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        if !alias.is_empty() {
            aliases.push((alias.to_string(), "builder_prefix"));
        }
    }
    for pair in words.windows(2) {
        if pair[0].text.eq_ignore_ascii_case("by")
            && pair[1].text.eq_ignore_ascii_case(builder_brand)
        {
            let alias =
                society_name[..pair[0].start].trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
            if !alias.is_empty() {
                aliases.push((alias.to_string(), "builder_byline"));
            }
            break;
        }
    }
    aliases
}

#[derive(Clone, Copy)]
struct WordSpan<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn word_spans(value: &str) -> Vec<WordSpan<'_>> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_ascii_alphanumeric() {
            start.get_or_insert(index);
        } else if let Some(word_start) = start.take() {
            words.push(WordSpan {
                text: &value[word_start..index],
                start: word_start,
                end: index,
            });
        }
    }
    if let Some(word_start) = start {
        words.push(WordSpan {
            text: &value[word_start..],
            start: word_start,
            end: value.len(),
        });
    }
    words
}

fn first_word(value: &str) -> Option<&str> {
    word_spans(value).first().map(|word| word.text)
}

struct AliasCollisionVocabulary {
    exact_phrases: HashSet<String>,
    protected_single_tokens: HashSet<String>,
    generic_tokens: HashSet<String>,
}

impl AliasCollisionVocabulary {
    fn new(entities: &[ServingEntityRecord]) -> Self {
        let mut exact_phrases = HashSet::new();
        let mut protected_single_tokens = HashSet::new();
        for entity in entities {
            let normalized = normalize_alias(&entity.name);
            if !normalized.is_empty() {
                exact_phrases.insert(normalized.clone());
            }
            if ["area", "place", "builder"]
                .iter()
                .any(|entity_type| entity.entity_type.eq_ignore_ascii_case(entity_type))
            {
                protected_single_tokens
                    .extend(normalized.split_whitespace().map(ToString::to_string));
                protected_single_tokens.extend(
                    normalize_alias(&entity.searchable_text)
                        .split_whitespace()
                        .map(ToString::to_string),
                );
            }
        }
        for area in area_alias_entries() {
            for value in std::iter::once(&area.canonical).chain(&area.aliases) {
                let normalized = normalize_alias(value);
                exact_phrases.insert(normalized.clone());
                protected_single_tokens
                    .extend(normalized.split_whitespace().map(ToString::to_string));
            }
        }
        let resolution = search_resolution_config();
        let mut generic_tokens = HashSet::new();
        let mut add_generic_value = |value: &str| {
            let normalized = normalize_alias(value);
            if !normalized.is_empty() {
                exact_phrases.insert(normalized.clone());
                generic_tokens.extend(normalized.split_whitespace().map(ToString::to_string));
            }
        };
        for value in resolution
            .ignored_entity_names
            .iter()
            .chain(&resolution.generic_scope_nouns)
            .chain(&resolution.mechanical_alias_blocked_tokens)
            .chain(schema::query_stopwords())
            .chain(schema::scoring_stopwords())
        {
            add_generic_value(value);
        }
        for family in &resolution.place_families {
            add_generic_value(&family.id);
            add_generic_value(&family.label);
            for alias in &family.aliases {
                add_generic_value(alias);
            }
        }
        let ranking_policy = schema::ranking_policy();
        for value in ranking_policy
            .named_place_generic_tokens
            .iter()
            .chain(&ranking_policy.named_place_query_stopwords)
        {
            add_generic_value(value);
        }
        Self {
            exact_phrases,
            protected_single_tokens,
            generic_tokens,
        }
    }

    fn collides(&self, alias: &str) -> bool {
        if self.exact_phrases.contains(alias) {
            return true;
        }
        let tokens = alias.split_whitespace().collect::<Vec<_>>();
        tokens.len() == 1
            && (self.protected_single_tokens.contains(tokens[0])
                || self.generic_tokens.contains(tokens[0]))
    }
}

#[derive(Debug)]
pub enum ServingEntityAliasError {
    Invalid(String),
}

impl fmt::Display for ServingEntityAliasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => {
                write!(formatter, "invalid serving entity aliases: {message}")
            }
        }
    }
}

impl std::error::Error for ServingEntityAliasError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(id: &str, entity_type: &str, name: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: id.to_string(),
            entity_type: entity_type.to_string(),
            name: name.to_string(),
            root_source: None,
            searchable_text: name.to_string(),
        }
    }

    fn built_by(society_id: &str, builder_id: &str) -> ServingEdgeRecord {
        ServingEdgeRecord {
            from_entity_id: society_id.to_string(),
            edge_type: "built_by".to_string(),
            to_entity_id: builder_id.to_string(),
            confidence: 1.0,
            source_type: "Rera".to_string(),
        }
    }

    #[test]
    fn materializes_builder_prefix_and_byline_aliases() {
        let entities = vec![
            entity("society:waterford", "society", "Prestige Waterford"),
            entity("society:folium-i", "society", "FOLIUM BY SUMADHURA PHASE-I"),
            entity(
                "society:folium-ii",
                "society",
                "FOLIUM BY SUMADHURA PHASE-II",
            ),
            entity(
                "society:folium-iii",
                "society",
                "FOLIUM BY SUMADHURA PHASE-III",
            ),
            entity(
                "society:folium-iv",
                "society",
                "FOLIUM BY SUMADHURA PHASE-IV",
            ),
            entity("builder:prestige", "builder", "Prestige Estates"),
            entity("builder:sumadhura", "builder", "Sumadhura Infracon"),
        ];
        let edges = vec![
            built_by("society:waterford", "builder:prestige"),
            built_by("society:folium-i", "builder:sumadhura"),
            built_by("society:folium-ii", "builder:sumadhura"),
            built_by("society:folium-iii", "builder:sumadhura"),
            built_by("society:folium-iv", "builder:sumadhura"),
        ];

        let aliases = materialize_society_aliases(&entities, &edges).unwrap();

        assert!(aliases
            .iter()
            .any(|alias| { alias.alias == "Waterford" && alias.entity_id == "society:waterford" }));
        let folium_aliases = aliases
            .iter()
            .filter(|alias| alias.normalized_alias == "folium")
            .collect::<Vec<_>>();
        assert_eq!(folium_aliases.len(), 4);
        let index = ServingEntityAliasIndex::from_records(aliases).unwrap();
        assert_eq!(index.get("Folium").unwrap().members.len(), 4);
    }

    #[test]
    fn rejects_area_collisions_and_ambiguous_aliases() {
        let entities = vec![
            entity("society:central", "society", "Century Central"),
            entity("society:tech-park", "society", "Prestige Tech Park"),
            entity("society:first", "society", "Prestige Waterford"),
            entity("society:second", "society", "Acme Waterford"),
            entity("builder:century", "builder", "Century Real Estate"),
            entity("builder:prestige", "builder", "Prestige Estates"),
            entity("builder:acme", "builder", "Acme Homes"),
        ];
        let edges = vec![
            built_by("society:central", "builder:century"),
            built_by("society:tech-park", "builder:prestige"),
            built_by("society:first", "builder:prestige"),
            built_by("society:second", "builder:acme"),
        ];

        let aliases = materialize_society_aliases(&entities, &edges).unwrap();

        assert!(aliases.is_empty());
    }

    #[test]
    fn materialized_aliases_round_trip_through_parquet() {
        let records = vec![ServingEntityAliasRecord {
            alias: "Folium".to_string(),
            normalized_alias: "folium".to_string(),
            entity_id: "society:folium".to_string(),
            entity_type: "society".to_string(),
            entity_name: "FOLIUM BY SUMADHURA PHASE-I".to_string(),
            source: "builder_byline".to_string(),
        }];

        let bytes = crate::serving::parquet::write_entity_aliases_parquet(&records).unwrap();
        let restored = crate::serving::parquet::read_entity_aliases_parquet(&bytes).unwrap();

        assert_eq!(restored, records);
    }
}
