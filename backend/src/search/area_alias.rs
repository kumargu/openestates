use std::sync::OnceLock;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, TantivyDocument, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, ReloadPolicy};

use crate::dag_config::area_alias_entries;

const FUZZY_ALIAS_LIMIT: usize = 3;
const MIN_FUZZY_TOKEN_CHARS: usize = 4;
const LONG_FUZZY_TOKEN_CHARS: usize = 7;
const AREA_FUZZY_STOPWORDS: &[&str] = &[
    "home",
    "homes",
    "house",
    "houses",
    "flat",
    "flats",
    "apartment",
    "apartments",
    "near",
    "with",
    "from",
    "under",
    "over",
    "good",
    "road",
    "roads",
    "access",
    "approach",
];

struct AreaAliasSearchIndex {
    index: Index,
    canonical: Field,
    alias: Field,
}

pub fn resolve_area_with_tantivy(query: &str, excluded_areas: &[String]) -> Option<String> {
    alias_index().resolve(query, excluded_areas)
}

fn alias_index() -> &'static AreaAliasSearchIndex {
    static INDEX: OnceLock<AreaAliasSearchIndex> = OnceLock::new();
    INDEX.get_or_init(AreaAliasSearchIndex::build)
}

impl AreaAliasSearchIndex {
    fn build() -> Self {
        let mut schema_builder = tantivy::schema::Schema::builder();
        let canonical = schema_builder.add_text_field("canonical", STRING | STORED);
        let alias = schema_builder.add_text_field("alias", TEXT | STORED);
        let schema = schema_builder.build();
        let index = Index::create_in_ram(schema);
        let mut writer = index
            .writer(16_000_000)
            .expect("in-memory Tantivy area alias writer should initialize");

        for entry in area_alias_entries() {
            writer
                .add_document(doc!(
                    canonical => entry.canonical.as_str(),
                    alias => entry.canonical.as_str(),
                ))
                .expect("area canonical alias should index");
            for alias_value in &entry.aliases {
                writer
                    .add_document(doc!(
                        canonical => entry.canonical.as_str(),
                        alias => alias_value.as_str(),
                    ))
                    .expect("area alias should index");
            }
        }
        writer
            .commit()
            .expect("in-memory Tantivy area alias index should commit");

        Self {
            index,
            canonical,
            alias,
        }
    }

    fn resolve(&self, query: &str, excluded_areas: &[String]) -> Option<String> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .ok()?;
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.alias]);
        let mut best: Option<(String, f32)> = None;

        for fuzzy_token in fuzzy_tokens(query) {
            let (parsed_query, _errors) = query_parser.parse_query_lenient(&fuzzy_token);
            let top_docs = searcher
                .search(
                    &parsed_query,
                    &TopDocs::with_limit(FUZZY_ALIAS_LIMIT).order_by_score(),
                )
                .ok()?;
            for (score, address) in top_docs {
                let doc: TantivyDocument = searcher.doc(address).ok()?;
                let canonical = stored_string(&doc, self.canonical)?;
                if excluded_areas
                    .iter()
                    .any(|excluded| excluded.eq_ignore_ascii_case(&canonical))
                {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|(_, best_score)| score > *best_score)
                {
                    best = Some((canonical, score));
                }
            }
        }

        best.map(|(canonical, _)| canonical)
            .or_else(|| resolve_area_with_edit_distance(query, excluded_areas))
    }
}

fn resolve_area_with_edit_distance(query: &str, excluded_areas: &[String]) -> Option<String> {
    let query_tokens = query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| token.len() >= MIN_FUZZY_TOKEN_CHARS && !is_fuzzy_stopword(token))
        .collect::<Vec<_>>();
    let mut best: Option<(String, usize, usize)> = None;

    for entry in area_alias_entries() {
        if excluded_areas
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(&entry.canonical))
        {
            continue;
        }
        for alias_value in std::iter::once(entry.canonical.as_str())
            .chain(entry.aliases.iter().map(String::as_str))
        {
            for alias_token in alias_value
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .map(|token| token.trim().to_ascii_lowercase())
                .filter(|token| token.len() >= MIN_FUZZY_TOKEN_CHARS)
            {
                let max_distance = if alias_token.len() >= LONG_FUZZY_TOKEN_CHARS {
                    3
                } else {
                    1
                };
                for query_token in &query_tokens {
                    if let Some(distance) =
                        edit_distance_at_most(query_token, &alias_token, max_distance)
                    {
                        let candidate = (entry.canonical.clone(), distance, alias_token.len());
                        if best.as_ref().is_none_or(|(_, best_distance, best_len)| {
                            distance < *best_distance
                                || (distance == *best_distance && alias_token.len() > *best_len)
                        }) {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
    }

    best.map(|(canonical, _, _)| canonical)
}

fn edit_distance_at_most(left: &str, right: &str, max_distance: usize) -> Option<usize> {
    if left.len().abs_diff(right.len()) > max_distance {
        return None;
    }
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.chars().enumerate() {
        current[0] = left_index + 1;
        let mut row_min = current[0];
        for (right_index, right_char) in right.chars().enumerate() {
            let substitution_cost = usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
            row_min = row_min.min(current[right_index + 1]);
        }
        if row_min > max_distance {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let distance = previous[right.len()];
    (distance <= max_distance).then_some(distance)
}

fn fuzzy_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            if token.len() < MIN_FUZZY_TOKEN_CHARS || is_fuzzy_stopword(&token) {
                return None;
            }
            let distance = if token.len() >= LONG_FUZZY_TOKEN_CHARS {
                2
            } else {
                1
            };
            Some(format!("{token}~{distance}"))
        })
        .collect()
}

fn is_fuzzy_stopword(token: &str) -> bool {
    AREA_FUZZY_STOPWORDS.contains(&token)
}

fn stored_string(doc: &TantivyDocument, field: Field) -> Option<String> {
    doc.get_first(field)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_tokens_use_tantivy_syntax() {
        assert_eq!(
            fuzzy_tokens("3bhk kadudgi under 2cr"),
            vec!["3bhk~1".to_string(), "kadudgi~2".to_string()]
        );
    }

    #[test]
    fn generic_home_words_do_not_match_area_aliases() {
        assert_eq!(
            resolve_area_with_tantivy("peaceful home near hospital", &[]),
            None
        );
    }

    #[test]
    fn resolves_kadugodi_typo_through_tantivy_alias_index() {
        assert_eq!(
            resolve_area_with_tantivy("3bhk kadudgi under 2cr", &[]).as_deref(),
            Some("Whitefield")
        );
    }
}
