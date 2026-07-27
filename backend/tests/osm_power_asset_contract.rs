use std::collections::BTreeMap;

use backend::assets::{
    osm_power_line_facts_input, KgViewRecords, OsmPowerInfrastructureInput,
    OsmPowerLineObservationRecord,
};
use backend::knowledge::KnowledgeGraph;
use chrono::{TimeZone, Utc};

#[test]
fn osm_power_line_facts_emit_transmission_red_flag_and_geometry() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 27, 9, 0, 0).unwrap();
    let input = OsmPowerInfrastructureInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![
            OsmPowerLineObservationRecord {
                entity_id: "society:prestige-southern-star".to_string(),
                project_key: None,
                query: "power=line around Prestige Southern Star".to_string(),
                osm_id: "way/12345".to_string(),
                name: Some("220 kV Somanahalli line".to_string()),
                power: "line".to_string(),
                voltage_kv: Some(220.0),
                distance_meters: 0.0,
                subject_latitude: Some(12.915),
                subject_longitude: Some(77.585),
                latitude: 12.915,
                longitude: 77.585,
                geometry_geojson:
                    r#"{"type":"LineString","coordinates":[[77.58,12.91],[77.59,12.92]]}"#
                        .to_string(),
                source_tags: BTreeMap::from([
                    ("power".to_string(), "line".to_string()),
                    ("voltage".to_string(), "220000".to_string()),
                ]),
                source_url: Some("https://www.openstreetmap.org/way/12345".to_string()),
                confidence: 0.86,
                fetched_at,
                fetch_source: "overpass_power_snapshot".to_string(),
            },
            OsmPowerLineObservationRecord {
                entity_id: "society:prestige-southern-star".to_string(),
                project_key: None,
                query: "power=line around Prestige Southern Star".to_string(),
                osm_id: "way/low-voltage".to_string(),
                name: Some("Local distribution line".to_string()),
                power: "line".to_string(),
                voltage_kv: Some(11.0),
                distance_meters: 0.0,
                subject_latitude: Some(12.91),
                subject_longitude: Some(77.58),
                latitude: 12.91,
                longitude: 77.58,
                geometry_geojson:
                    r#"{"type":"LineString","coordinates":[[77.58,12.91],[77.581,12.911]]}"#
                        .to_string(),
                source_tags: BTreeMap::new(),
                source_url: Some("https://www.openstreetmap.org/way/low-voltage".to_string()),
                confidence: 0.7,
                fetched_at,
                fetch_source: "overpass_power_snapshot".to_string(),
            },
        ],
        source_watermarks: Vec::new(),
    };

    let facts = osm_power_line_facts_input(&input, "test-run").unwrap();

    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:prestige-southern-star"
            && fact.fact_key == "high_voltage_transmission_line_nearby"
            && fact.value_json.contains("220 kV Somanahalli line")
            && fact.value_json.contains("severity: critical")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:prestige-southern-star"
            && fact.fact_key == "high_voltage_transmission_line_place_entity"
            && fact.value_json.contains("place:osm-power-line:way-12345")
    }));
    assert!(!facts
        .facts
        .iter()
        .any(|fact| fact.value_json.contains("Local distribution line")));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "place:osm-power-line:way-12345"
            && fact.fact_key == "geo.geometry_geojson"
            && fact.value_json.contains("LineString")
    }));
    assert!(facts.fact_annotations.iter().any(|annotation| {
        annotation.fact_key == "high_voltage_transmission_line_nearby"
            && annotation
                .answers_preferences_json
                .contains("avoid transmission line")
    }));

    let kg_records = KgViewRecords::from_graph_with_skill_facts(
        &KnowledgeGraph::new(),
        &facts.facts,
        &facts.fact_annotations,
    )
    .unwrap();
    assert!(kg_records.entities.iter().any(|entity| {
        entity.entity_id == "place:osm-power-line:way-12345" && entity.entity_type == "place"
    }));
    assert!(kg_records.facts.iter().any(|fact| {
        fact.entity_id == "place:osm-power-line:way-12345"
            && fact.fact_key == "geo.geometry_geojson"
    }));
}

#[test]
fn osm_power_line_facts_reject_invalid_geometry_and_distance_mismatch() {
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 27, 9, 0, 0).unwrap();
    let base = OsmPowerLineObservationRecord {
        entity_id: "society:prestige-southern-star".to_string(),
        project_key: None,
        query: "power=line around Prestige Southern Star".to_string(),
        osm_id: "way/12345".to_string(),
        name: Some("220 kV Somanahalli line".to_string()),
        power: "line".to_string(),
        voltage_kv: Some(220.0),
        distance_meters: 82.0,
        subject_latitude: Some(12.915),
        subject_longitude: Some(77.585),
        latitude: 12.915,
        longitude: 77.585,
        geometry_geojson: r#"{"type":"LineString","coordinates":[[77.58,12.91],[77.59,12.92]]}"#
            .to_string(),
        source_tags: BTreeMap::new(),
        source_url: Some("https://www.openstreetmap.org/way/12345".to_string()),
        confidence: 0.86,
        fetched_at,
        fetch_source: "overpass_power_snapshot".to_string(),
    };

    let invalid_geojson = OsmPowerInfrastructureInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![OsmPowerLineObservationRecord {
            geometry_geojson: "not geojson".to_string(),
            ..base.clone()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(osm_power_line_facts_input(&invalid_geojson, "test-run").is_err());

    let wrong_distance = OsmPowerInfrastructureInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![OsmPowerLineObservationRecord {
            distance_meters: 5_000.0,
            ..base.clone()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(osm_power_line_facts_input(&wrong_distance, "test-run").is_err());

    let missing_subject = OsmPowerInfrastructureInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![OsmPowerLineObservationRecord {
            subject_latitude: None,
            subject_longitude: None,
            ..base.clone()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(osm_power_line_facts_input(&missing_subject, "test-run").is_err());

    let polygon = OsmPowerInfrastructureInput {
        snapshot_date: "2026-07-27".to_string(),
        records: vec![OsmPowerLineObservationRecord {
            distance_meters: 0.0,
            geometry_geojson:
                r#"{"type":"Polygon","coordinates":[[[77.58,12.91],[77.59,12.91],[77.59,12.92],[77.58,12.91]]]}"#
                    .to_string(),
            ..base
        }],
        source_watermarks: Vec::new(),
    };
    assert!(osm_power_line_facts_input(&polygon, "test-run").is_err());
}
