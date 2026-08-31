use backend::assets::{
    osm_society_access_facts_input, KgViewRecords, OsmSocietyAccessInput, OsmSocietyAccessRecord,
};
use backend::knowledge::KnowledgeGraph;
use chrono::{TimeZone, Utc};

fn waterford_access() -> OsmSocietyAccessRecord {
    OsmSocietyAccessRecord {
        entity_id: "society:prestige-waterford".to_string(),
        project_key: None,
        query: "OSM society boundary, gates and public roads".to_string(),
        access_id: "waterford-access".to_string(),
        approach_road_name: Some("ECC Road".to_string()),
        approach_way_id: Some("23213668".to_string()),
        approach_distance_meters: Some(640.0),
        approach_geometry_geojson: Some(
            r#"{"type":"LineString","coordinates":[[77.739,12.979],[77.742,12.982],[77.744,12.983]]}"#.to_string(),
        ),
        approach_source_geometry_geojson: Some(
            r#"{"type":"LineString","coordinates":[[77.738,12.978],[77.742,12.982],[77.745,12.984]]}"#.to_string(),
        ),
        approach_direction: Some("two_way".to_string()),
        approach_association_method: Some("address_match".to_string()),
        boundary_name: Some("Prestige Waterford".to_string()),
        boundary_way_id: Some("133630420".to_string()),
        boundary_geometry_geojson: Some(
            r#"{"type":"Polygon","coordinates":[[[77.740,12.980],[77.744,12.980],[77.744,12.983],[77.740,12.980]]]}"#.to_string(),
        ),
        entrance_id: Some("node/501".to_string()),
        entrance_latitude: Some(12.981),
        entrance_longitude: Some(77.7405),
        entrance_status: Some("inferred".to_string()),
        entrance_association_method: Some("osm_gate_boundary_public_road".to_string()),
        entrance_source_url: Some("https://www.openstreetmap.org/node/501".to_string()),
        subject_latitude: 12.9814204,
        subject_longitude: 77.7408686,
        source_url: Some("https://www.openstreetmap.org/way/23213668".to_string()),
        confidence: 0.78,
        fetched_at: Utc.with_ymd_and_hms(2026, 8, 30, 9, 0, 0).unwrap(),
        fetch_source: "overpass_society_access_snapshot".to_string(),
    }
}

#[test]
fn society_access_emits_boundary_public_corridor_and_typed_entrance() {
    let facts = osm_society_access_facts_input(
        &OsmSocietyAccessInput {
            snapshot_date: "2026-08-30".to_string(),
            records: vec![waterford_access()],
            source_watermarks: Vec::new(),
        },
        "test-run",
    )
    .unwrap();

    assert!(facts.facts.iter().all(|fact| {
        fact.fact_key != "transit_access_route"
            && fact.fact_key != "transit_access_route_entity"
            && !fact.value_json.contains("ground_access")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:prestige-waterford"
            && fact.fact_key == "society.boundary_geojson"
            && fact.source_url.as_deref() == Some("https://www.openstreetmap.org/way/133630420")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "approach_road_entity" && fact.value_json.contains("place:approach-road:")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id.starts_with("place:approach-road:")
            && fact.fact_key == "road.direction"
            && fact.value_json.contains("two_way")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "society.entrance_entity"
            && fact.value_json.contains("place:society-entrance:")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id.starts_with("place:society-entrance:")
            && fact.fact_key == "entrance.status"
            && fact.value_json.contains("inferred")
            && fact.source_url.as_deref() == Some("https://www.openstreetmap.org/node/501")
    }));

    let kg_records = KgViewRecords::from_graph_with_skill_facts(
        &KnowledgeGraph::new(),
        &facts.facts,
        &facts.fact_annotations,
    )
    .unwrap();
    assert!(kg_records.entities.iter().any(|entity| {
        entity.entity_id.starts_with("place:approach-road:") && entity.entity_type == "place"
    }));
    assert!(kg_records.entities.iter().any(|entity| {
        entity.entity_id.starts_with("place:society-entrance:") && entity.entity_type == "place"
    }));
}

#[test]
fn missing_entrance_emits_no_entity_or_claim() {
    let mut record = waterford_access();
    record.entrance_id = None;
    record.entrance_latitude = None;
    record.entrance_longitude = None;
    record.entrance_status = None;
    record.entrance_association_method = None;
    record.entrance_source_url = None;
    let facts = osm_society_access_facts_input(
        &OsmSocietyAccessInput {
            snapshot_date: "2026-08-30".to_string(),
            records: vec![record],
            source_watermarks: Vec::new(),
        },
        "test-run",
    )
    .unwrap();
    assert!(facts.facts.iter().all(|fact| {
        fact.fact_key != "society.entrance_entity"
            && !fact.entity_id.starts_with("place:society-entrance:")
    }));
}

#[test]
fn invalid_public_corridor_geometry_is_rejected() {
    let input = OsmSocietyAccessInput {
        snapshot_date: "2026-08-30".to_string(),
        records: vec![OsmSocietyAccessRecord {
            approach_geometry_geojson: Some(
                r#"{"type":"LineString","coordinates":[[77.74,12.98]]}"#.to_string(),
            ),
            ..waterford_access()
        }],
        source_watermarks: Vec::new(),
    };
    assert!(osm_society_access_facts_input(&input, "test-run").is_err());
}
