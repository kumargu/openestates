use backend::knowledge::fact::{
    FactSource, FactValue, ScoringDirection, ScoringHint, SourceType, SourcedFact,
};
use backend::knowledge::graph::KnowledgeGraph;
use backend::knowledge::node::{Node, NodeType, RootSource};
use backend::lake::{LakeKey, LakeStore};
use backend::serving::{
    hydrate_tantivy_index, read_edges_parquet, read_entities_parquet, read_facts_parquet,
    read_search_metadata_parquet, BundleArtifactKind, ServingBundleBuilder, ServingBundleLoader,
    ServingBundleManifest, ServingEntityRecord, ServingEntityVisibility, ServingFactRecord,
    ServingSearchMetadataRecord, SourceObservation, TantivyRecallIndex,
};
use chrono::Utc;
use parquet::file::reader::{FileReader, SerializedFileReader};
use std::fs::File;
use tempfile::tempdir;

#[tokio::test]
async fn serving_bundle_writes_parquet_manifest_and_hydratable_tantivy_index() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let graph = mock_graph();
    let (entities, facts, metadata) = catalog_records_from_graph(&graph);

    let manifest = ServingBundleBuilder::new(lake.clone())
        .build_from_catalog_records(
            entities,
            facts,
            metadata,
            Vec::new(),
            Vec::new(),
            "2026-07-12T18:30Z",
        )
        .await
        .unwrap();

    assert_eq!(manifest.entity_count, 2);
    assert_eq!(manifest.fact_count, 18);
    assert_eq!(manifest.search_metadata_count, 18);
    assert_eq!(manifest.rera_evidence_count, 0);
    assert_eq!(manifest.edge_count, 0);
    assert_eq!(manifest.eligibility_policy_version, 5);
    assert_eq!(manifest.quarantined_society_count, 0);
    assert_eq!(
        manifest.entity_parquet_key,
        "serving/search_bundle/version=2026-07-12t18-30z/entities/part-00000.parquet"
    );
    assert_eq!(
        manifest.fact_parquet_key,
        "serving/search_bundle/version=2026-07-12t18-30z/facts/part-00000.parquet"
    );
    assert_eq!(
        manifest.search_metadata_parquet_key,
        "serving/search_bundle/version=2026-07-12t18-30z/search_metadata/part-00000.parquet"
    );
    assert!(manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == BundleArtifactKind::TantivyIndexFile));
    assert!(manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == BundleArtifactKind::ReraEvidenceParquet));
    assert!(manifest
        .artifacts
        .iter()
        .any(|artifact| artifact.kind == BundleArtifactKind::QuarantineJson));
    let mut legacy_manifest = serde_json::to_value(&manifest).unwrap();
    legacy_manifest
        .as_object_mut()
        .unwrap()
        .insert("trust_policy_key".to_string(), serde_json::json!("retired"));
    assert!(serde_json::from_value::<ServingBundleManifest>(legacy_manifest).is_err());
    let topology_gaps = manifest
        .artifacts
        .iter()
        .find(|artifact| {
            artifact
                .key
                .ends_with("diagnostics/spatial_topology_gaps.json")
        })
        .expect("spatial topology gaps remain inspectable in the bundle");
    let topology_gap_body = lake
        .get_text(&LakeKey::new(topology_gaps.key.clone()).unwrap())
        .await
        .unwrap();
    let topology_gap_json: serde_json::Value = serde_json::from_str(&topology_gap_body).unwrap();
    assert_eq!(topology_gap_json["classification"], "data_gap");

    let entity_bytes = lake
        .get_bytes(&LakeKey::new(manifest.entity_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let fact_bytes = lake
        .get_bytes(&LakeKey::new(manifest.fact_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let search_metadata_bytes = lake
        .get_bytes(&LakeKey::new(manifest.search_metadata_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    let edge_bytes = lake
        .get_bytes(&LakeKey::new(manifest.edge_parquet_key.clone()).unwrap())
        .await
        .unwrap();
    assert_is_parquet(&entity_bytes);
    assert_is_parquet(&fact_bytes);
    assert_is_parquet(&search_metadata_bytes);
    assert_eq!(parquet_rows(&entity_bytes), 2);
    assert_eq!(parquet_rows(&fact_bytes), 18);
    assert_eq!(parquet_rows(&search_metadata_bytes), 18);
    let fact_columns = parquet_columns(&fact_bytes);
    let search_metadata_columns = parquet_columns(&search_metadata_bytes);
    let edge_columns = parquet_columns(&edge_bytes);
    assert!(fact_columns.contains(&"value_text".to_string()));
    assert!(fact_columns.contains(&"value_number".to_string()));
    assert!(fact_columns.contains(&"value_tags".to_string()));
    assert!(!fact_columns.contains(&"value_json".to_string()));
    assert!(!fact_columns.contains(&"answers_preferences_json".to_string()));
    assert!(search_metadata_columns.contains(&"answers_preferences".to_string()));
    assert!(!search_metadata_columns.contains(&"answers_preferences_json".to_string()));
    assert!(edge_columns.contains(&"derivation_json".to_string()));

    let fact_records = read_facts_parquet(&fact_bytes).unwrap();
    let land_area = fact_records
        .iter()
        .find(|fact| {
            fact.entity_id == "society:green-acre-whitefield"
                && fact.fact_key == "rera_total_land_area_sqm"
        })
        .unwrap();
    assert_eq!(land_area.value, FactValue::Numeric(48_000.0));

    let search_metadata_records = read_search_metadata_parquet(&search_metadata_bytes).unwrap();
    let greenery_metadata = search_metadata_records
        .iter()
        .find(|metadata| {
            metadata.entity_id == "society:green-acre-whitefield"
                && metadata.fact_key == "resident_greenery_signal"
        })
        .unwrap();
    assert!(greenery_metadata
        .answers_preferences
        .contains(&"greenery".to_string()));

    let manifest_key =
        LakeKey::new("serving/search_bundle/version=2026-07-12t18-30z/manifest.json").unwrap();
    let manifest_body = lake.get_text(&manifest_key).await.unwrap();
    let manifest_json: serde_json::Value = serde_json::from_str(&manifest_body).unwrap();
    assert_eq!(manifest_json["format_version"], 12);
    assert!(manifest_json["entity_alias_parquet_key"]
        .as_str()
        .is_some_and(|key| key.ends_with("entity_aliases/part-00000.parquet")));

    let schema_key =
        LakeKey::new("serving/search_bundle/version=2026-07-12t18-30z/schema.json").unwrap();
    let schema_body = lake.get_text(&schema_key).await.unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema_body).unwrap();
    assert_eq!(schema_json["storage_format"], "parquet+tantivy");
    assert!(schema_body.contains("\"entity_aliases\""));
    assert!(schema_body.contains("\"value_number\""));
    assert!(schema_body.contains("\"answers_preferences\""));
    assert!(schema_body.contains("\"observation_json\""));
    assert!(schema_body.contains("\"derivation_json\""));
    assert!(!schema_body.contains("value_json"));
    assert!(!schema_body.contains("answers_preferences_json"));

    let hydrated = tempdir().unwrap();
    hydrate_tantivy_index(&lake, &manifest, hydrated.path())
        .await
        .unwrap();
    let recall = TantivyRecallIndex::open(hydrated.path()).unwrap();
    let hits = recall.search("whitefield greenery trees", 5).unwrap();
    assert_eq!(hits[0].entity_id, "society:green-acre-whitefield");
    assert_eq!(hits[0].entity_type, "society");
    assert_eq!(hits[0].name, "Green Acre Whitefield");
    assert!(hits[0].matched_fields.iter().any(|field| field == "name"));
    assert!(hits[0].matched_fields.iter().any(|field| field == "body"));
}

#[tokio::test]
async fn normal_serving_build_materializes_internal_geo_cells_and_excludes_their_names() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let version = "geo-cell-normal-dag-fixture";
    let entities = vec![
        serving_entity(
            "area:market:whitefield",
            "area",
            "Whitefield",
            "market_locality",
        ),
        serving_entity(
            "area:osm:cell-a",
            "area",
            "Secret Ward Alpha",
            "openstreetmap",
        ),
        serving_entity(
            "area:osm:cell-b",
            "area",
            "Secret Ward Beta",
            "openstreetmap",
        ),
        serving_entity("place:school", "place", "Fixture School", "google"),
    ];
    let learned_at = Utc::now();
    let facts = vec![
        observed_serving_fact(
            "area:market:whitefield",
            "market.locality_name",
            FactValue::Text("Whitefield".to_string()),
            "SourceEntitySeed",
            "market-name",
            learned_at,
        ),
        observed_serving_fact(
            "area:osm:cell-a",
            "place.name",
            FactValue::Text("Secret Ward Alpha".to_string()),
            "OpenStreetMap",
            "cell-a-name",
            learned_at,
        ),
        observed_serving_fact(
            "area:osm:cell-a",
            "area.admin_level",
            FactValue::Text("10".to_string()),
            "OpenStreetMap",
            "cell-a-level",
            learned_at,
        ),
        observed_serving_fact(
            "area:osm:cell-a",
            "geo.geometry_geojson",
            FactValue::Text(
                r#"{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}"#
                    .to_string(),
            ),
            "OpenStreetMap",
            "cell-a-geometry",
            learned_at,
        ),
        observed_serving_fact(
            "area:osm:cell-b",
            "place.name",
            FactValue::Text("Secret Ward Beta".to_string()),
            "OpenStreetMap",
            "cell-b-name",
            learned_at,
        ),
        observed_serving_fact(
            "area:osm:cell-b",
            "area.admin_level",
            FactValue::Text("10".to_string()),
            "OpenStreetMap",
            "cell-b-level",
            learned_at,
        ),
        observed_serving_fact(
            "area:osm:cell-b",
            "geo.geometry_geojson",
            FactValue::Text(
                r#"{"type":"Polygon","coordinates":[[[10,0],[20,0],[20,10],[10,10],[10,0]]]}"#
                    .to_string(),
            ),
            "OpenStreetMap",
            "cell-b-geometry",
            learned_at,
        ),
        observed_serving_fact(
            "place:school",
            "google_place_address",
            FactValue::Text("Main Road, Whitefield, Bengaluru".to_string()),
            "Google",
            "school-address",
            learned_at,
        ),
        observed_serving_fact(
            "place:school",
            "geo.latitude",
            FactValue::Numeric(2.0),
            "Google",
            "school-coordinate",
            learned_at,
        ),
        observed_serving_fact(
            "place:school",
            "geo.longitude",
            FactValue::Numeric(2.0),
            "Google",
            "school-coordinate",
            learned_at,
        ),
    ];
    let metadata = facts.iter().map(serving_metadata).collect();

    let manifest = ServingBundleBuilder::new(lake.clone())
        .build_from_catalog_records(entities, facts, metadata, Vec::new(), Vec::new(), version)
        .await
        .unwrap();
    let cache = tempdir().unwrap();
    let bundle = ServingBundleLoader::new(lake.clone(), cache.path())
        .load_search_bundle(version)
        .await
        .unwrap();

    for cell_id in ["area:osm:cell-a", "area:osm:cell-b"] {
        let cell = bundle
            .entities
            .iter()
            .find(|entity| entity.entity_id == cell_id)
            .unwrap();
        assert_eq!(cell.visibility, ServingEntityVisibility::Internal);
        assert!(cell.searchable_text.is_empty());
    }
    assert!(bundle.edges.iter().any(|edge| {
        edge.from_entity_id == "place:school"
            && edge.edge_type == "in_market_locality"
            && edge.to_entity_id == "area:market:whitefield"
            && edge.derivation.is_some()
    }));
    assert!(bundle.edges.iter().any(|edge| {
        edge.from_entity_id == "place:school"
            && edge.edge_type == "occupies_geo_cell"
            && edge.to_entity_id == "area:osm:cell-a"
            && edge.derivation.is_some()
    }));
    assert!(bundle.edges.iter().any(|edge| {
        edge.from_entity_id == "area:market:whitefield"
            && edge.edge_type == "covers_geo_cell"
            && edge.to_entity_id == "area:osm:cell-a"
            && edge.derivation.is_some()
    }));
    let hydrated = tempdir().unwrap();
    hydrate_tantivy_index(&lake, &manifest, hydrated.path())
        .await
        .unwrap();
    assert!(TantivyRecallIndex::open(hydrated.path())
        .unwrap()
        .search("Secret Ward Alpha", 10)
        .unwrap()
        .is_empty());

    let entity_bytes = lake
        .get_bytes(&LakeKey::new(manifest.entity_parquet_key).unwrap())
        .await
        .unwrap();
    assert_eq!(
        read_entities_parquet(&entity_bytes).unwrap(),
        bundle.entities
    );
    let edge_bytes = lake
        .get_bytes(&LakeKey::new(manifest.edge_parquet_key).unwrap())
        .await
        .unwrap();
    let edges = read_edges_parquet(&edge_bytes).unwrap();
    assert!(edges
        .iter()
        .filter_map(|edge| edge.derivation.as_ref())
        .all(|derivation| derivation
            .input_evidence
            .iter()
            .all(|evidence| evidence.snapshot_identity == version)));
}

fn serving_entity(
    entity_id: &str,
    entity_type: &str,
    name: &str,
    root_source: &str,
) -> ServingEntityRecord {
    ServingEntityRecord {
        entity_id: entity_id.to_string(),
        entity_type: entity_type.to_string(),
        name: name.to_string(),
        root_source: Some(root_source.to_string()),
        visibility: ServingEntityVisibility::Searchable,
        searchable_text: name.to_string(),
    }
}

fn observed_serving_fact(
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    source_type: &str,
    provider_id: &str,
    learned_at: chrono::DateTime<Utc>,
) -> ServingFactRecord {
    let value_type = match value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Bool(_) => "bool",
        FactValue::Tags(_) => "tags",
        FactValue::Score { .. } => "score",
        FactValue::Text(_) => "text",
    };
    let value_text = match &value {
        FactValue::Text(value) => Some(value.clone()),
        FactValue::Numeric(value) => Some(value.to_string()),
        FactValue::Bool(value) => Some(value.to_string()),
        _ => None,
    };
    ServingFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_text,
        value,
        confidence: 0.95,
        source_type: source_type.to_string(),
        source_url: Some("https://example.test/source".to_string()),
        model: None,
        skill_id: Some("normal-dag-fixture".to_string()),
        learned_at,
        observation: Some(
            SourceObservation::new(
                source_type,
                provider_id,
                entity_id,
                learned_at,
                Some("https://example.test/source".to_string()),
                vec!["asset:normal-dag-fixture/run:1".to_string()],
            )
            .unwrap(),
        ),
    }
}

fn serving_metadata(fact: &ServingFactRecord) -> ServingSearchMetadataRecord {
    ServingSearchMetadataRecord {
        entity_id: fact.entity_id.clone(),
        fact_key: fact.fact_key.clone(),
        display_template: None,
        answers_preferences: Vec::new(),
        scoring_direction: None,
        scoring_weight: None,
        scoring_thresholds: Vec::new(),
    }
}

#[test]
fn serving_manifest_requires_the_complete_format_12_contract() {
    let old_manifest = r#"{"bundle_version":"old","format_version":5}"#;
    assert!(serde_json::from_str::<ServingBundleManifest>(old_manifest).is_err());
}

fn catalog_records_from_graph(
    graph: &KnowledgeGraph,
) -> (
    Vec<ServingEntityRecord>,
    Vec<ServingFactRecord>,
    Vec<ServingSearchMetadataRecord>,
) {
    let mut entities = Vec::new();
    let mut facts = Vec::new();
    let mut metadata = Vec::new();
    for node in graph.nodes.values() {
        entities.push(ServingEntityRecord {
            entity_id: node.id.clone(),
            entity_type: "society".to_string(),
            name: node.name.clone(),
            root_source: node.root_source.map(|_| "rera".to_string()),
            visibility: ServingEntityVisibility::Searchable,
            searchable_text: String::new(),
        });
        let mut current = std::collections::BTreeMap::new();
        for sourced in &node.facts {
            current
                .entry(sourced.key.as_str())
                .and_modify(|existing: &mut &SourcedFact| {
                    if sourced.version > existing.version {
                        *existing = sourced;
                    }
                })
                .or_insert(sourced);
        }
        for sourced in current.into_values() {
            let value_type = match &sourced.value {
                FactValue::Numeric(_) => "numeric",
                FactValue::Text(_) => "text",
                FactValue::Bool(_) => "bool",
                FactValue::Tags(_) => "tags",
                FactValue::Score { .. } => "score",
            };
            facts.push(ServingFactRecord {
                entity_id: node.id.clone(),
                fact_key: sourced.key.clone(),
                value_type: value_type.to_string(),
                value_text: None,
                value: sourced.value.clone(),
                confidence: sourced.confidence,
                source_type: format!("{:?}", sourced.source.source_type),
                source_url: sourced.source.url.clone(),
                model: sourced.source.model.clone(),
                skill_id: sourced.source.skill_id.clone(),
                learned_at: sourced.learned_at,
                observation: None,
            });
            metadata.push(ServingSearchMetadataRecord {
                entity_id: node.id.clone(),
                fact_key: sourced.key.clone(),
                display_template: sourced.display_template.clone(),
                answers_preferences: sourced.answers_preferences.clone(),
                scoring_direction: sourced
                    .scoring_hint
                    .as_ref()
                    .map(|_| "text_match".to_string()),
                scoring_weight: sourced.scoring_hint.as_ref().map(|hint| hint.weight),
                scoring_thresholds: sourced
                    .scoring_hint
                    .as_ref()
                    .map(|hint| hint.thresholds.clone())
                    .unwrap_or_default(),
            });
        }
    }
    (entities, facts, metadata)
}

fn mock_graph() -> KnowledgeGraph {
    let mut graph = KnowledgeGraph::new();

    let mut green = Node::new(
        "society:green-acre-whitefield",
        NodeType::Society,
        "Green Acre Whitefield",
    );
    green.root_source = Some(RootSource::Rera);
    green.add_fact(fact(
        "rera_total_land_area_sqm",
        FactValue::Numeric(40_000.0),
        SourceType::Rera,
        &["large campus", "above 10 acres"],
    ));
    let mut corrected_land_area = fact(
        "rera_total_land_area_sqm",
        FactValue::Numeric(48_000.0),
        SourceType::Rera,
        &["large campus", "above 10 acres"],
    );
    corrected_land_area.version = 2;
    green.add_fact(corrected_land_area);
    green.add_fact(fact(
        "resident_greenery_signal",
        FactValue::Text(
            "Residents mention many trees, calm internal roads, and open space".to_string(),
        ),
        SourceType::Reddit,
        &["greenery", "trees", "calm layout"],
    ));
    add_serving_eligibility_facts(&mut green, "Whitefield", "/media/green.webp");

    let mut dense = Node::new(
        "society:dense-tower-whitefield",
        NodeType::Society,
        "Dense Tower Whitefield",
    );
    dense.root_source = Some(RootSource::Rera);
    dense.add_fact(fact(
        "rera_total_land_area_sqm",
        FactValue::Numeric(8_000.0),
        SourceType::Rera,
        &["compact project"],
    ));
    dense.add_fact(fact(
        "resident_traffic_signal",
        FactValue::Text("Residents complain about congestion near the gate".to_string()),
        SourceType::Reddit,
        &["traffic", "congestion"],
    ));
    add_serving_eligibility_facts(&mut dense, "Whitefield", "/media/dense.webp");

    graph.add_node(green);
    graph.add_node(dense);
    graph
}

fn add_serving_eligibility_facts(node: &mut Node, area: &str, hero_image: &str) {
    for (key, value) in [
        ("rera_registered", FactValue::Bool(true)),
        (
            "approach_road_condition",
            FactValue::Text("documented".to_string()),
        ),
        ("area", FactValue::Text(area.to_string())),
        ("builder_name", FactValue::Text("Test Builder".to_string())),
        (
            "listing_3bhk",
            FactValue::Text(
                serde_json::json!({"price": 12_000_000.0, "area_sqft": 1_200.0}).to_string(),
            ),
        ),
        ("hero_image", FactValue::Text(hero_image.to_string())),
        ("images", FactValue::Tags(vec![hero_image.to_string()])),
    ] {
        node.add_fact(fact(key, value, SourceType::Rera, &[]));
    }
}

fn fact(
    key: &str,
    value: FactValue,
    source_type: SourceType,
    answers_preferences: &[&str],
) -> SourcedFact {
    SourcedFact {
        key: key.to_string(),
        value,
        confidence: match source_type {
            SourceType::Rera => 1.0,
            SourceType::Reddit => 0.7,
            _ => 0.6,
        },
        source: FactSource {
            source_type,
            url: None,
            model: None,
            skill_id: None,
            triggered_by: None,
        },
        learned_at: Utc::now(),
        version: 1,
        display_template: Some(format!("{key}: {{value}}")),
        answers_preferences: answers_preferences
            .iter()
            .map(|value| value.to_string())
            .collect(),
        scoring_hint: Some(ScoringHint {
            direction: ScoringDirection::TextMatch,
            weight: 1.0,
            thresholds: Vec::new(),
        }),
    }
}

fn assert_is_parquet(bytes: &[u8]) {
    assert!(bytes.len() > 8);
    assert_eq!(&bytes[..4], b"PAR1");
    assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
}

fn parquet_rows(bytes: &[u8]) -> i64 {
    let dir = tempdir().unwrap();
    let path = dir.path().join("table.parquet");
    std::fs::write(&path, bytes).unwrap();
    let file = File::open(path).unwrap();
    let reader = SerializedFileReader::new(file).unwrap();
    reader.metadata().file_metadata().num_rows()
}

fn parquet_columns(bytes: &[u8]) -> Vec<String> {
    let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        bytes::Bytes::copy_from_slice(bytes),
    )
    .unwrap();
    reader
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}
