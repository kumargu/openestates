use backend::assets::{
    osm_transit_access_corridor_facts_input, KgViewRecords, OsmTransitAccessCorridorRecord,
    OsmTransitAccessCorridorsInput,
};
use backend::knowledge::KnowledgeGraph;
use chrono::{TimeZone, Utc};

fn waterford_corridor() -> OsmTransitAccessCorridorRecord {
    OsmTransitAccessCorridorRecord {
        entity_id: "society:prestige-waterford".to_string(),
        project_key: None,
        query: "OSM streets between Waterford and the nearest operational metro".to_string(),
        corridor_id: "waterford-tree-park".to_string(),
        destination_station_id: "node/123".to_string(),
        destination_name: "Kadugodi Tree Park".to_string(),
        destination_latitude: 12.9855,
        destination_longitude: 77.7475,
        frontage_road_name: Some("ECC Road".to_string()),
        frontage_way_id: Some("23213668".to_string()),
        frontage_distance_meters: Some(640.0),
        frontage_geometry_geojson: Some(
            r#"{"type":"LineString","coordinates":[[77.739,12.979],[77.742,12.982],[77.744,12.983]]}"#
                .to_string(),
        ),
        road_names: vec![
            "Ecumenical Christian Center Road".to_string(),
            "Whitefield Main Road".to_string(),
        ],
        route_way_ids: vec!["23213668".to_string()],
        distance_meters: 1_120.0,
        origin_snap_distance_meters: 42.0,
        destination_snap_distance_meters: 18.0,
        subject_latitude: 12.9814204,
        subject_longitude: 77.7408686,
        geometry_geojson:
            r#"{"type":"LineString","coordinates":[[77.7409,12.9814],[77.744,12.983],[77.7475,12.9855]]}"#
                .to_string(),
        source_url: Some("https://www.openstreetmap.org/way/23213668".to_string()),
        confidence: 0.78,
        fetched_at: Utc.with_ymd_and_hms(2026, 8, 30, 9, 0, 0).unwrap(),
        fetch_source: "overpass_access_corridor_snapshot".to_string(),
    }
}

#[test]
fn osm_access_facts_emit_a_linked_routed_corridor() {
    let input = OsmTransitAccessCorridorsInput {
        snapshot_date: "2026-08-30".to_string(),
        records: vec![waterford_corridor()],
        source_watermarks: Vec::new(),
    };

    let facts = osm_transit_access_corridor_facts_input(&input, "test-run").unwrap();
    let route_entity_id = facts
        .facts
        .iter()
        .find(|fact| fact.fact_key == "transit_access_route_entity")
        .expect("linked route entity fact")
        .value_json
        .clone();

    assert!(route_entity_id.contains("place:transit-access:"));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "transit_access_route"
            && fact.value_json.contains("ECC Road → Kadugodi Tree Park")
            && fact.value_json.contains("1.1 km")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "geo.geometry_geojson"
            && fact.value_json.contains("LineString")
            && fact.source_type == "OpenStreetMap"
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "route.frontage_road_name" && fact.value_json.contains("ECC Road")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:prestige-waterford"
            && fact.fact_key == "approach_road"
            && fact.value_json.contains("ECC Road")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "approach_road_entity" && fact.value_json.contains("place:approach-road:")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id.starts_with("place:approach-road:")
            && fact.fact_key == "geo.geometry_geojson"
            && fact.value_json.contains("77.739")
    }));

    let kg_records = KgViewRecords::from_graph_with_skill_facts(
        &KnowledgeGraph::new(),
        &facts.facts,
        &facts.fact_annotations,
    )
    .unwrap();
    assert!(kg_records.entities.iter().any(|entity| {
        entity.entity_id.starts_with("place:transit-access:") && entity.entity_type == "place"
    }));
    assert!(kg_records.entities.iter().any(|entity| {
        entity.entity_id.starts_with("place:approach-road:") && entity.entity_type == "place"
    }));
}

#[test]
fn osm_access_facts_reject_invalid_route_geometry() {
    let input = OsmTransitAccessCorridorsInput {
        snapshot_date: "2026-08-30".to_string(),
        records: vec![OsmTransitAccessCorridorRecord {
            geometry_geojson: r#"{"type":"LineString","coordinates":[[77.74,12.98]]}"#.to_string(),
            ..waterford_corridor()
        }],
        source_watermarks: Vec::new(),
    };

    assert!(osm_transit_access_corridor_facts_input(&input, "test-run").is_err());
}
