use std::sync::OnceLock;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, TantivyDocument, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, ReloadPolicy};

use crate::dag_config::area_alias_entries;

const FUZZY_ALIAS_LIMIT: usize = 3;
const MIN_FUZZY_TOKEN_CHARS: usize = 4;
const LONG_FUZZY_TOKEN_CHARS: usize = 7;

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
            .writer(10_000_000)
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
    }
}

fn fuzzy_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            if token.len() < MIN_FUZZY_TOKEN_CHARS {
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
            vec![
                "3bhk~1".to_string(),
                "kadudgi~2".to_string(),
                "under~1".to_string()
            ]
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
