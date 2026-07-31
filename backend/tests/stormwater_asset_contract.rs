use std::collections::BTreeMap;

use backend::assets::{
    stormwater_drain_facts_input, KgViewRecords, StormwaterDrainObservationRecord,
    StormwaterDrainRiskInput,
};
use backend::knowledge::KnowledgeGraph;
use chrono::{TimeZone, Utc};

#[test]
fn stormwater_drain_facts_emit_rajakaluve_risk_and_geometry() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 27, 11, 0, 0).unwrap();
    let input = StormwaterDrainRiskInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![
            StormwaterDrainObservationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "OpenCity stormwater drains around Green Acre".to_string(),
                drain_id: "swd/rajakaluve-123".to_string(),
                name: Some("Varthur Rajakaluve".to_string()),
                drain_type: "rajakaluve".to_string(),
                hierarchy: Some("primary_swd".to_string()),
                distance_meters: 0.0,
                intersects_property: false,
                subject_latitude: Some(12.941),
                subject_longitude: Some(77.746),
                latitude: 12.941,
                longitude: 77.746,
                geometry_geojson:
                    r#"{"type":"LineString","coordinates":[[77.745,12.94],[77.747,12.942]]}"#
                        .to_string(),
                encroachment_record: Some("BBMP notified SWD survey record".to_string()),
                source_tags: BTreeMap::from([
                    ("source".to_string(), "OpenCity".to_string()),
                    ("drain_type".to_string(), "rajakaluve".to_string()),
                ]),
                source_url: Some(
                    "https://data.opencity.in/dataset/bengaluru-stormwater-drains-maps".to_string(),
                ),
                source_type: Some("OpenStreetMap".to_string()),
                confidence: 0.84,
                fetched_at,
                fetch_source: "opencity_stormwater_drain_snapshot".to_string(),
            },
            StormwaterDrainObservationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "OpenCity stormwater drains around Green Acre".to_string(),
                drain_id: "swd/primary-rajakaluve-tagged".to_string(),
                name: Some("Tagged primary drain".to_string()),
                drain_type: "primary_swd".to_string(),
                hierarchy: Some("primary_swd".to_string()),
                distance_meters: 0.0,
                intersects_property: false,
                subject_latitude: Some(12.943),
                subject_longitude: Some(77.748),
                latitude: 12.943,
                longitude: 77.748,
                geometry_geojson:
                    r#"{"type":"LineString","coordinates":[[77.748,12.943],[77.749,12.944]]}"#
                        .to_string(),
                encroachment_record: None,
                source_tags: BTreeMap::from([(
                    "local_name".to_string(),
                    "Rajakaluve primary SWD".to_string(),
                )]),
                source_url: Some(
                    "https://data.opencity.in/dataset/bengaluru-stormwater-drains-maps".to_string(),
                ),
                source_type: Some("OpenStreetMap".to_string()),
                confidence: 0.8,
                fetched_at,
                fetch_source: "opencity_stormwater_drain_snapshot".to_string(),
            },
            StormwaterDrainObservationRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "OpenCity stormwater drains around Green Acre".to_string(),
                drain_id: "swd/far-999".to_string(),
                name: Some("Far SWD".to_string()),
                drain_type: "stormwater_drain".to_string(),
                hierarchy: Some("tertiary_swd".to_string()),
                distance_meters: 600.0,
                intersects_property: false,
                subject_latitude: Some(12.9446),
                subject_longitude: Some(77.75),
                latitude: 12.95,
                longitude: 77.75,
                geometry_geojson:
                    r#"{"type":"LineString","coordinates":[[77.75,12.95],[77.751,12.951]]}"#
                        .to_string(),
                encroachment_record: None,
                source_tags: BTreeMap::new(),
                source_url: Some(
                    "https://data.opencity.in/dataset/bengaluru-stormwater-drains-maps".to_string(),
                ),
                source_type: Some("OpenCity".to_string()),
                confidence: 0.8,
                fetched_at,
                fetch_source: "opencity_stormwater_drain_snapshot".to_string(),
            },
        ],
        source_watermarks: Vec::new(),
    };

    let facts = stormwater_drain_facts_input(&input, "test-run").unwrap();

    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:green-acre-whitefield"
            && fact.fact_key == "stormwater_drain_nearby"
            && fact.value_json.contains("Varthur Rajakaluve")
            && fact.value_json.contains("severity: critical")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:green-acre-whitefield"
            && fact.fact_key == "stormwater_drain_place_entity"
            && fact
                .value_json
                .contains("place:stormwater-drain:swd-rajakaluve-123")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "rajakaluve_nearby" && fact.value_json.contains("Varthur Rajakaluve")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "rajakaluve_nearby" && fact.value_json.contains("Tagged primary drain")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "rajakaluve_encroachment_record"
            && fact.value_json.contains("BBMP notified SWD survey record")
    }));
    assert!(!facts
        .facts
        .iter()
        .any(|fact| fact.value_json.contains("Far SWD")));

    let kg_records = KgViewRecords::from_graph_with_skill_facts(
        &KnowledgeGraph::new(),
        &facts.facts,
        &facts.fact_annotations,
    )
    .unwrap();
    assert!(kg_records.entities.iter().any(|entity| {
        entity.entity_id == "place:stormwater-drain:swd-rajakaluve-123"
            && entity.entity_type == "place"
    }));
    assert!(kg_records.facts.iter().any(|fact| {
        fact.entity_id == "place:stormwater-drain:swd-rajakaluve-123"
            && fact.fact_key == "geo.geometry_geojson"
    }));
}

#[test]
fn stormwater_drain_facts_reject_invalid_geometry_and_distance_mismatch() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 27, 11, 0, 0).unwrap();
    let base = StormwaterDrainObservationRecord {
        entity_id: "society:green-acre-whitefield".to_string(),
        project_key: None,
        query: "OpenCity stormwater drains around Green Acre".to_string(),
        drain_id: "swd/rajakaluve-123".to_string(),
        name: Some("Varthur Rajakaluve".to_string()),
        drain_type: "rajakaluve".to_string(),
        hierarchy: Some("primary_swd".to_string()),
        distance_meters: 42.0,
        intersects_property: false,
        subject_latitude: Some(12.94),
        subject_longitude: Some(77.745),
        latitude: 12.941,
        longitude: 77.746,
        geometry_geojson: r#"{"type":"LineString","coordinates":[[77.745,12.94],[77.747,12.942]]}"#
            .to_string(),
        encroachment_record: None,
        source_tags: BTreeMap::new(),
        source_url: None,
        source_type: Some("OpenCity".to_string()),
        confidence: 0.84,
        fetched_at,
        fetch_source: "opencity_stormwater_drain_snapshot".to_string(),
    };

    let invalid_geojson = StormwaterDrainRiskInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![StormwaterDrainObservationRecord {
            geometry_geojson: "not geojson".to_string(),
            ..base.clone()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(stormwater_drain_facts_input(&invalid_geojson, "test-run").is_err());

    let wrong_distance = StormwaterDrainRiskInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![StormwaterDrainObservationRecord {
            distance_meters: 5_000.0,
            ..base.clone()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(stormwater_drain_facts_input(&wrong_distance, "test-run").is_err());

    let missing_subject = StormwaterDrainRiskInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![StormwaterDrainObservationRecord {
            subject_latitude: None,
            subject_longitude: None,
            ..base.clone()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(stormwater_drain_facts_input(&missing_subject, "test-run").is_err());

    let polygon = StormwaterDrainRiskInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![StormwaterDrainObservationRecord {
            distance_meters: 0.0,
            geometry_geojson:
                r#"{"type":"Polygon","coordinates":[[[77.745,12.94],[77.747,12.94],[77.747,12.942],[77.745,12.94]]]}"#
                    .to_string(),
            ..base
        }],
        source_watermarks: Vec::new(),
    };
    assert!(stormwater_drain_facts_input(&polygon, "test-run").is_err());
}
