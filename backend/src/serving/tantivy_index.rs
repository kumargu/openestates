use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, TantivyDocument, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexWriter, ReloadPolicy};

use crate::lake::{LakeKey, LakeStore};

use super::{
    ServingBundleManifest, ServingEntityRecord, ServingFactRecord, ServingSearchMetadataRecord,
};

pub struct TantivyRecallIndex {
    index: Index,
    entity_id: Field,
    name: Field,
    body: Field,
    fact_keys: Field,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TantivyRecallHit {
    pub entity_id: String,
    pub score: f32,
}

impl TantivyRecallIndex {
    pub fn build_in_dir(
        path: impl AsRef<Path>,
        entities: &[ServingEntityRecord],
        facts: &[ServingFactRecord],
        search_metadata: &[ServingSearchMetadataRecord],
    ) -> Result<Self, TantivyIndexError> {
        std::fs::create_dir_all(path.as_ref()).map_err(TantivyIndexError::Io)?;

        let mut schema_builder = tantivy::schema::Schema::builder();
        let entity_id = schema_builder.add_text_field("entity_id", STRING | STORED);
        let entity_type = schema_builder.add_text_field("entity_type", STRING);
        let name = schema_builder.add_text_field("name", TEXT | STORED);
        let body = schema_builder.add_text_field("body", TEXT);
        let fact_keys = schema_builder.add_text_field("fact_keys", TEXT);
        let schema = schema_builder.build();

        let index =
            Index::create_in_dir(path, schema.clone()).map_err(TantivyIndexError::Tantivy)?;
        let mut writer: IndexWriter = index
            .writer(50_000_000)
            .map_err(TantivyIndexError::Tantivy)?;
        let facts_by_entity = facts_for_entities(facts, search_metadata);

        for entity in entities {
            let fact_text = facts_by_entity
                .get(&entity.entity_id)
                .cloned()
                .unwrap_or_default();
            writer
                .add_document(tantivy::doc!(
                    entity_id => entity.entity_id.as_str(),
                    entity_type => entity.entity_type.as_str(),
                    name => entity.name.as_str(),
                    body => format!("{} {}", entity.searchable_text, fact_text.body).as_str(),
                    fact_keys => fact_text.keys.as_str(),
                ))
                .map_err(TantivyIndexError::Tantivy)?;
        }

        writer.commit().map_err(TantivyIndexError::Tantivy)?;
        Ok(Self {
            index,
            entity_id,
            name,
            body,
            fact_keys,
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, TantivyIndexError> {
        let index = Index::open_in_dir(path).map_err(TantivyIndexError::Tantivy)?;
        let schema = index.schema();
        let entity_id = schema
            .get_field("entity_id")
            .map_err(TantivyIndexError::Tantivy)?;
        let name = schema
            .get_field("name")
            .map_err(TantivyIndexError::Tantivy)?;
        let body = schema
            .get_field("body")
            .map_err(TantivyIndexError::Tantivy)?;
        let fact_keys = schema
            .get_field("fact_keys")
            .map_err(TantivyIndexError::Tantivy)?;
        Ok(Self {
            index,
            entity_id,
            name,
            body,
            fact_keys,
        })
    }

    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TantivyRecallHit>, TantivyIndexError> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(TantivyIndexError::Tantivy)?;
        let searcher = reader.searcher();
        let mut query_parser =
            QueryParser::for_index(&self.index, vec![self.name, self.body, self.fact_keys]);
        query_parser.set_conjunction_by_default();
        let (query, _errors) = query_parser.parse_query_lenient(query);
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit).order_by_score())
            .map_err(TantivyIndexError::Tantivy)?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, address) in top_docs {
            let doc: TantivyDocument = searcher.doc(address).map_err(TantivyIndexError::Tantivy)?;
            let entity_id = doc
                .get_first(self.entity_id)
                .and_then(|value| value.as_str())
                .ok_or(TantivyIndexError::MissingEntityId)?
                .to_string();
            hits.push(TantivyRecallHit { entity_id, score });
        }
        Ok(hits)
    }
}

pub async fn hydrate_tantivy_index(
    lake: &LakeStore,
    manifest: &ServingBundleManifest,
    target_dir: impl AsRef<Path>,
) -> Result<(), TantivyIndexError> {
    let target_dir = target_dir.as_ref();
    std::fs::create_dir_all(target_dir).map_err(TantivyIndexError::Io)?;
    let prefix = format!("{}/", manifest.tantivy_index_prefix.trim_end_matches('/'));

    for artifact in manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == super::BundleArtifactKind::TantivyIndexFile)
    {
        let key = LakeKey::new(artifact.key.clone()).map_err(TantivyIndexError::Key)?;
        let bytes = lake
            .get_bytes(&key)
            .await
            .map_err(TantivyIndexError::Lake)?;
        let relative = artifact
            .key
            .strip_prefix(&prefix)
            .ok_or_else(|| TantivyIndexError::InvalidManifestKey(artifact.key.clone()))?;
        let path = target_dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(TantivyIndexError::Io)?;
        }
        std::fs::write(path, bytes).map_err(TantivyIndexError::Io)?;
    }

    Ok(())
}

#[derive(Clone, Default)]
struct EntityFactText {
    body: String,
    keys: String,
}

fn facts_for_entities(
    facts: &[ServingFactRecord],
    search_metadata: &[ServingSearchMetadataRecord],
) -> HashMap<String, EntityFactText> {
    let mut by_entity = HashMap::<String, EntityFactText>::new();
    for fact in facts {
        let entry = by_entity.entry(fact.entity_id.clone()).or_default();
        entry.keys.push(' ');
        entry.keys.push_str(&fact.fact_key);
        entry.body.push(' ');
        entry.body.push_str(&fact.fact_key);
        if let Some(value) = &fact.value_text {
            entry.body.push(' ');
            entry.body.push_str(value);
        }
    }

    for metadata in search_metadata {
        let entry = by_entity.entry(metadata.entity_id.clone()).or_default();
        if let Some(display_template) = &metadata.display_template {
            entry.body.push(' ');
            entry.body.push_str(display_template);
        }
        entry.body.push(' ');
        entry.body.push_str(&metadata.answers_preferences_json);
    }
    by_entity
}

#[derive(Debug)]
pub enum TantivyIndexError {
    Io(std::io::Error),
    Key(crate::lake::keys::KeyError),
    Lake(crate::lake::LakeError),
    Tantivy(tantivy::TantivyError),
    InvalidManifestKey(String),
    MissingEntityId,
}

impl fmt::Display for TantivyIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "Tantivy index IO error: {err}"),
            Self::Key(err) => write!(f, "Tantivy index lake key error: {err}"),
            Self::Lake(err) => write!(f, "Tantivy index lake error: {err}"),
            Self::Tantivy(err) => write!(f, "Tantivy index error: {err}"),
            Self::InvalidManifestKey(key) => {
                write!(f, "Tantivy artifact key is outside manifest prefix: {key}")
            }
            Self::MissingEntityId => f.write_str("Tantivy document missing stored entity_id"),
        }
    }
}

impl std::error::Error for TantivyIndexError {}
