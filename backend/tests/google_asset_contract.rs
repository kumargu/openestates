use backend::assets::{
    canonicalize_google_places_input, google_nearby_place_facts_input, google_review_facts_input,
    read_google_nearby_place_rows, read_google_place_rows, AssetPartition,
    CanonicalSocietyMaterializer, GoogleNearbyPlaceRecord, GoogleNearbyPlacesWeeklyInput,
    GooglePlaceSnapshotMaterializer, GooglePlaceSnapshotRecord, GooglePlacesWeeklyInput,
    MaterializationId, ReraProjectSnapshotRecord, ReraRegistryMaterializer,
    ReraRegistryMonthlyInput,
};
use backend::lake::LakeStore;
use chrono::{TimeZone, Utc};
use tempfile::tempdir;

#[tokio::test]
async fn google_place_snapshot_materializes_raw_parquet_and_derives_linked_facts() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let input = GooglePlacesWeeklyInput {
        snapshot_date: "2026-07-14".to_string(),
        records: vec![GooglePlaceSnapshotRecord {
            entity_id: "society:green-acre-whitefield".to_string(),
            project_key: None,
            query: "Green Acre Whitefield Bengaluru".to_string(),
            place_name: Some("Green Acre".to_string()),
            place_id: Some("ChIJ-green-acre".to_string()),
            reviews_url: "https://www.google.com/maps/place/?q=place_id:ChIJ-green-acre"
                .to_string(),
            rating: Some(4.4),
            review_count: Some(812),
            review_snippets: vec![
                "Well maintained campus with good clubhouse.".to_string(),
                "Traffic near the approach road can be slow.".to_string(),
            ],
            address: Some("Whitefield, Bengaluru".to_string()),
            confidence: 0.9,
            fetched_at,
            fetch_source: "serpapi_google_maps".to_string(),
        }],
        source_watermarks: Vec::new(),
    };

    let materialization = GooglePlaceSnapshotMaterializer::new(lake.clone())
        .materialize_and_promote(&input, "google-contract-run")
        .await
        .unwrap();
    let rows = read_google_place_rows(&lake, &materialization.record)
        .await
        .unwrap();
    assert_eq!(rows, input.records);
    assert!(materialization
        .record
        .artifacts
        .iter()
        .any(|artifact| artifact
            .key
            .starts_with("raw/source=google/dt=2026-07-14/run_id=")
            && artifact.key.ends_with("places/part-00000.parquet")
            && !artifact.key.contains("/source=google/run_id=")));

    let facts =
        google_review_facts_input(&lake, &materialization.record, &MaterializationId::new())
            .await
            .unwrap();
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:green-acre-whitefield"
            && fact.fact_key == "google_reviews_url"
            && fact.source_url.as_deref()
                == Some("https://www.google.com/maps/place/?q=place_id:ChIJ-green-acre")
    }));
    assert!(facts
        .facts
        .iter()
        .any(|fact| { fact.fact_key == "google_rating" && fact.value_json.contains("4.4") }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "google_review_snippets"
            && fact.value_json.contains("Well maintained campus")
    }));
    assert!(facts.fact_annotations.iter().any(|annotation| {
        annotation.fact_key == "google_reviews_url"
            && annotation
                .answers_preferences_json
                .contains("resident reviews")
    }));
}

#[tokio::test]
async fn google_nearby_snapshot_materializes_raw_parquet_and_derives_category_facts() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let input = GoogleNearbyPlacesWeeklyInput {
        snapshot_date: "2026-07-14".to_string(),
        records: vec![
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "schools near Green Acre Whitefield Bengaluru".to_string(),
                category: "school".to_string(),
                place_name: "Greenwood High".to_string(),
                place_id: Some("school-1".to_string()),
                place_url: "https://www.google.com/maps/place/greenwood".to_string(),
                distance_km: Some(1.2),
                rating: Some(4.3),
                review_count: Some(420),
                primary_type: Some("school".to_string()),
                place_types: vec!["school".to_string()],
                confidence: 0.85,
                fetched_at,
                fetch_source: "serpapi_google_maps_nearby".to_string(),
            },
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "metro station near Green Acre Whitefield Bengaluru".to_string(),
                category: "metro".to_string(),
                place_name: "Kadugodi Tree Park Metro Station".to_string(),
                place_id: Some("metro-1".to_string()),
                place_url: "https://www.google.com/maps/place/kadugodi-metro".to_string(),
                distance_km: Some(2.1),
                rating: Some(4.2),
                review_count: Some(800),
                primary_type: Some("subway_station".to_string()),
                place_types: vec!["subway_station".to_string(), "transit_station".to_string()],
                confidence: 0.84,
                fetched_at,
                fetch_source: "google_places_text_search_nearby".to_string(),
            },
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "metro station near Green Acre Whitefield Bengaluru".to_string(),
                category: "metro".to_string(),
                place_name: "Varthuru Bus Stop".to_string(),
                place_id: Some("metro-bus-stop".to_string()),
                place_url: "https://www.google.com/maps/place/varthuru-bus-stop".to_string(),
                distance_km: Some(1.0),
                rating: Some(4.5),
                review_count: Some(29),
                primary_type: Some("bus_stop".to_string()),
                place_types: vec![
                    "bus_stop".to_string(),
                    "transit_station".to_string(),
                    "transit_stop".to_string(),
                ],
                confidence: 0.84,
                fetched_at,
                fetch_source: "google_places_text_search_nearby".to_string(),
            },
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "hospitals near Green Acre Whitefield Bengaluru".to_string(),
                category: "hospital".to_string(),
                place_name: "Manipal Hospital Whitefield".to_string(),
                place_id: Some("hospital-1".to_string()),
                place_url: "https://www.google.com/maps/place/manipal".to_string(),
                distance_km: Some(3.4),
                rating: Some(4.1),
                review_count: Some(1_200),
                primary_type: Some("hospital".to_string()),
                place_types: vec!["hospital".to_string()],
                confidence: 0.82,
                fetched_at,
                fetch_source: "serpapi_google_maps_nearby".to_string(),
            },
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "gym fitness near Green Acre Whitefield Bengaluru".to_string(),
                category: "fitness".to_string(),
                place_name: "Cult Whitefield".to_string(),
                place_id: Some("fitness-1".to_string()),
                place_url: "https://www.google.com/maps/place/cult-whitefield".to_string(),
                distance_km: Some(1.4),
                rating: Some(4.4),
                review_count: Some(980),
                primary_type: Some("gym".to_string()),
                place_types: vec!["gym".to_string()],
                confidence: 0.82,
                fetched_at,
                fetch_source: "google_places_text_search_nearby".to_string(),
            },
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "restaurant cafe near Green Acre Whitefield Bengaluru".to_string(),
                category: "eatery".to_string(),
                place_name: "Example Cafe".to_string(),
                place_id: Some("eatery-1".to_string()),
                place_url: "https://www.google.com/maps/place/example-cafe".to_string(),
                distance_km: Some(0.9),
                rating: Some(4.0),
                review_count: Some(300),
                primary_type: Some("cafe".to_string()),
                place_types: vec!["cafe".to_string(), "restaurant".to_string()],
                confidence: 0.82,
                fetched_at,
                fetch_source: "google_places_text_search_nearby".to_string(),
            },
            GoogleNearbyPlaceRecord {
                entity_id: "society:green-acre-whitefield".to_string(),
                project_key: None,
                query: "restaurant cafe near Green Acre Whitefield Bengaluru".to_string(),
                category: "eatery".to_string(),
                place_name: "Far Cafe".to_string(),
                place_id: Some("eatery-far".to_string()),
                place_url: "https://www.google.com/maps/place/far-cafe".to_string(),
                distance_km: Some(9.0),
                rating: Some(4.8),
                review_count: Some(10_000),
                primary_type: Some("cafe".to_string()),
                place_types: vec!["cafe".to_string()],
                confidence: 0.82,
                fetched_at,
                fetch_source: "google_places_text_search_nearby".to_string(),
            },
        ],
        source_watermarks: Vec::new(),
    };

    let materialization = GooglePlaceSnapshotMaterializer::new(lake.clone())
        .materialize_nearby_and_promote(&input, "google-nearby-contract-run")
        .await
        .unwrap();
    let rows = read_google_nearby_place_rows(&lake, &materialization.record)
        .await
        .unwrap();
    assert_eq!(rows, input.records);
    assert!(materialization
        .record
        .artifacts
        .iter()
        .any(|artifact| artifact
            .key
            .starts_with("raw/source=google/dt=2026-07-14/run_id=")
            && artifact.key.ends_with("nearby_places/part-00000.parquet")));

    let facts =
        google_nearby_place_facts_input(&lake, &materialization.record, &MaterializationId::new())
            .await
            .unwrap();
    assert!(facts.facts.iter().any(|fact| {
        fact.entity_id == "society:green-acre-whitefield"
            && fact.fact_key == "nearby_schools"
            && fact.source_url.as_deref() == Some("https://www.google.com/maps/place/greenwood")
            && fact.value_json.contains("Greenwood High")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "nearby_metro_stations"
            && fact.source_url.as_deref()
                == Some("https://www.google.com/maps/place/kadugodi-metro")
            && fact.value_json.contains("Kadugodi Tree Park")
    }));
    assert!(!facts
        .facts
        .iter()
        .any(|fact| fact.fact_key == "nearby_metro_stations"
            && fact.value_json.contains("Varthuru Bus Stop")));
    assert!(facts.fact_annotations.iter().any(|annotation| {
        annotation.fact_key == "nearby_hospitals"
            && annotation.answers_preferences_json.contains("hospital")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "nearby_fitness"
            && fact.source_url.as_deref()
                == Some("https://www.google.com/maps/place/cult-whitefield")
    }));
    assert!(facts.facts.iter().any(|fact| {
        fact.fact_key == "nearby_eateries"
            && fact.source_url.as_deref() == Some("https://www.google.com/maps/place/example-cafe")
    }));
    assert!(!facts
        .facts
        .iter()
        .any(|fact| fact.value_json.contains("Far Cafe")));
}

#[tokio::test]
async fn invalid_google_place_rows_do_not_replace_the_current_snapshot() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let materializer = GooglePlaceSnapshotMaterializer::new(lake.clone());
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let valid = GooglePlacesWeeklyInput {
        snapshot_date: "2026-07-14".to_string(),
        records: vec![GooglePlaceSnapshotRecord {
            entity_id: "society:rera-valid".to_string(),
            project_key: Some("PRM-VALID".to_string()),
            query: "Valid Society Bengaluru".to_string(),
            place_name: Some("Valid Society".to_string()),
            place_id: Some("valid-place".to_string()),
            reviews_url: "https://www.google.com/maps/place/valid".to_string(),
            rating: Some(4.2),
            review_count: Some(10),
            review_snippets: Vec::new(),
            address: None,
            confidence: 0.8,
            fetched_at,
            fetch_source: "contract".to_string(),
        }],
        source_watermarks: Vec::new(),
    };
    let current = materializer
        .materialize_and_promote(&valid, "valid-google")
        .await
        .unwrap();

    let mut invalid_inputs = Vec::new();
    let mut invalid_url = valid.clone();
    invalid_url.records[0].reviews_url = "javascript:alert(1)".to_string();
    invalid_inputs.push(invalid_url);
    let mut invalid_confidence = valid.clone();
    invalid_confidence.records[0].confidence = f32::NAN;
    invalid_inputs.push(invalid_confidence);
    let mut invalid_rating = valid.clone();
    invalid_rating.records[0].rating = Some(5.5);
    invalid_inputs.push(invalid_rating);

    for (index, input) in invalid_inputs.iter().enumerate() {
        assert!(materializer
            .materialize_and_promote(input, format!("invalid-google-{index}"))
            .await
            .is_err());
    }
    let pointer = backend::assets::AssetMaterializationStore::new(lake)
        .current_record(
            &backend::assets::AssetId::new("google_places_weekly").unwrap(),
            &backend::assets::AssetPartition::new([("source", "google")]),
        )
        .await
        .unwrap();
    assert_eq!(
        pointer.materialization_id,
        current.record.materialization_id
    );
}

#[tokio::test]
async fn google_place_alias_resolves_to_first_run_rera_canonical_entity() {
    let temp = tempdir().unwrap();
    let lake = LakeStore::local(temp.path()).unwrap();
    let fetched_at = Utc.with_ymd_and_hms(2026, 7, 14, 10, 0, 0).unwrap();
    let run_id = MaterializationId::new();
    let rera = ReraRegistryMaterializer::new(lake.clone())
        .materialize_for_run(
            &ReraRegistryMonthlyInput {
                snapshot_date: "2026-07".to_string(),
                projects: vec![ReraProjectSnapshotRecord {
                    ack_number: Some("ACK-1".to_string()),
                    registration_number: Some("PRM-1".to_string()),
                    project_name: "Prestige Raintree Park".to_string(),
                    promoter_name: Some("Prestige Group".to_string()),
                    status: None,
                    project_type: None,
                    project_address: None,
                    area_name: Some("Whitefield".to_string()),
                    district: None,
                    taluk: None,
                    total_land_area_sqm: None,
                    land_litigation: None,
                    source_url: "https://rera.example/project".to_string(),
                    fetched_at,
                }],
                detail_facts: Vec::new(),
                detail_fact_annotations: Vec::new(),
                source_watermarks: Vec::new(),
            },
            run_id.clone(),
            AssetPartition::global(),
        )
        .await
        .unwrap();
    let canonical = CanonicalSocietyMaterializer::new(lake.clone())
        .materialize_from_rera_for_run(&rera, "first-run", run_id, AssetPartition::global())
        .await
        .unwrap();
    let input = GooglePlacesWeeklyInput {
        snapshot_date: "2026-07-14".to_string(),
        records: vec![GooglePlaceSnapshotRecord {
            entity_id: "society:prestige-raintree-park".to_string(),
            project_key: None,
            query: "Prestige Raintree Park Whitefield Bengaluru".to_string(),
            place_name: Some("Prestige Raintree Park".to_string()),
            place_id: None,
            reviews_url: "https://www.google.com/maps/search/?api=1&query=raintree".to_string(),
            rating: None,
            review_count: None,
            review_snippets: Vec::new(),
            address: None,
            confidence: 0.6,
            fetched_at,
            fetch_source: "google_maps_search_fallback".to_string(),
        }],
        source_watermarks: Vec::new(),
    };

    let resolved = canonicalize_google_places_input(&lake, &input, &canonical)
        .await
        .unwrap();

    assert_ne!(resolved.records[0].entity_id, input.records[0].entity_id);
    assert!(resolved.records[0].entity_id.starts_with("society:rera-"));
}
