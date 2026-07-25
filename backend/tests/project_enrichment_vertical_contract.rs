use backend::assets::{
    default_openestates_registry, read_skill_fact_artifact_rows, AssetDagExecutionOptions,
    AssetDagExecutor, AssetMaterializationStore, AssetPartition, AssetSourceInputs,
    ExternalImageObservationRecord, ExternalImagesWeeklyInput, ExternalListingObservationRecord,
    ExternalListingsWeeklyInput, GoogleNearbyPlaceRecord, GoogleNearbyPlacesWeeklyInput,
    GooglePlaceSnapshotRecord, GooglePlacesWeeklyInput, ReraProjectSnapshotRecord,
    ReraRegistryMonthlyInput, SkillFactAnnotationRecord, SkillFactRecord, SourceWatermark,
    BUILDER_RERA_AGGREGATES_ASSET_ID, EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
    EXTERNAL_LISTING_FACTS_ASSET_ID,
};
use backend::knowledge::{FactValue, KnowledgeGraph};
use backend::lake::LakeStore;
use backend::serving::ServingBundleLoader;
use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[tokio::test]
async fn three_societies_reach_serving_with_listing_and_builder_evidence() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let observed_at = Utc.with_ymd_and_hms(2026, 7, 14, 12, 0, 0).unwrap();
    let projects = fixtures();
    let partition = AssetPartition::new([("dt", "2026-07-14")]);

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(partition, observed_at)
                .with_version("2026-07-14T12:00Z")
                .with_source_inputs(source_inputs(&projects, observed_at)),
        )
        .await
        .unwrap();

    for asset_id in [
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
        EXTERNAL_LISTING_FACTS_ASSET_ID,
        BUILDER_RERA_AGGREGATES_ASSET_ID,
        "kg_society_view",
        "search_serving_bundle",
    ] {
        assert!(
            report
                .executed_assets
                .iter()
                .any(|executed| executed.as_str() == asset_id),
            "{asset_id} should execute"
        );
    }
    let listings_record = store
        .current_record(
            &asset_id(EXTERNAL_LISTINGS_WEEKLY_ASSET_ID),
            &AssetPartition::new([("source", "external_listing")]),
        )
        .await
        .unwrap();
    assert_eq!(listings_record.row_count, 3);
    assert!(listings_record.artifacts.iter().any(|artifact| {
        artifact.content_type == "application/vnd.apache.parquet"
            && artifact.key.ends_with("listings/part-00000.parquet")
    }));

    let builder_record = store
        .current_record(
            &asset_id(BUILDER_RERA_AGGREGATES_ASSET_ID),
            &AssetPartition::global(),
        )
        .await
        .unwrap();
    let builder_asset_rows = read_skill_fact_artifact_rows(&lake, &[builder_record])
        .await
        .unwrap();
    assert!(builder_asset_rows
        .facts
        .iter()
        .any(|fact| fact.entity_id == "builder:prestige-estates-projects-limited"));

    let loaded = ServingBundleLoader::new(lake, root.path().join("serving-cache"))
        .load_current_search_bundle()
        .await
        .unwrap()
        .unwrap();
    for project in &projects {
        let alias = format!("society:{}", slug(project.name));
        let rows = loaded.fact_index.entity(&alias).unwrap();
        for fact_key in [
            "rera_number",
            "listing_3bhk",
            "listing_price_3bhk",
            "listing_price_range_3bhk",
            "listing_area_sqft_3bhk",
            "listing_area_sqft_range_3bhk",
            "listing_price_per_sqft_range_3bhk",
            "listing_area_type_3bhk",
            "listing_source_url_3bhk",
            "listing_source_name_3bhk",
        ] {
            assert!(
                rows.facts.iter().any(|fact| fact.fact_key == fact_key),
                "{alias} should contain {fact_key}"
            );
        }
        let listing_source_name = rows
            .facts
            .iter()
            .find(|fact| fact.fact_key == "listing_source_name_3bhk")
            .expect("external listing source name should reach serving");
        assert_eq!(
            listing_source_name.value,
            FactValue::Text("MagicBricks".to_string())
        );
    }

    let builder_rows = loaded
        .fact_index
        .entity("builder:prestige-estates-projects-limited")
        .unwrap();
    assert!(builder_rows.facts.iter().any(|fact| {
        fact.fact_key == "builder_project_count" && fact.value == FactValue::Numeric(3.0)
    }));
    assert!(builder_rows
        .facts
        .iter()
        .any(|fact| fact.fact_key == "builder_rera_status_breakdown"));
}

struct ProjectFixture {
    name: &'static str,
    registration: &'static str,
    status: &'static str,
    acres: f64,
    price: f64,
    latitude: f64,
    longitude: f64,
}

fn fixtures() -> Vec<ProjectFixture> {
    vec![
        ProjectFixture {
            name: "Prestige Raintree Park",
            registration: "PRM/KA/RERA/1251/446/PR/270824/006981",
            status: "Under Construction",
            acres: 21.0,
            price: 37_000_000.0,
            latitude: 12.95384,
            longitude: 77.74546,
        },
        ProjectFixture {
            name: "Prestige Park Grove",
            registration: "PRM/KA/RERA/1251/446/PR/100823/006141",
            status: "Sold Out",
            acres: 71.41,
            price: 36_000_000.0,
            latitude: 13.01903,
            longitude: 77.75745,
        },
        ProjectFixture {
            name: "Prestige Lavender Fields",
            registration: "PRM/KA/RERA/1251/446/PR/290423/005906",
            status: "Sold Out",
            acres: 18.2,
            price: 24_300_000.0,
            latitude: 12.93405,
            longitude: 77.73904,
        },
    ]
}

fn source_inputs(
    projects: &[ProjectFixture],
    observed_at: chrono::DateTime<Utc>,
) -> AssetSourceInputs {
    let mut detail_facts = Vec::new();
    let mut detail_fact_annotations = Vec::new();
    let rera_projects = projects
        .iter()
        .map(|project| {
            let canonical = canonical_id(project.registration);
            let alias = format!("society:{}", slug(project.name));
            for entity_id in [canonical, alias] {
                detail_facts.push(SkillFactRecord {
                    entity_id: entity_id.clone(),
                    fact_key: "rera_lat_lng".to_string(),
                    value_type: "text".to_string(),
                    value_json: serde_json::to_string(&FactValue::Text(format!(
                        "{},{}",
                        project.latitude, project.longitude
                    )))
                    .unwrap(),
                    confidence: 1.0,
                    source_type: "Rera".to_string(),
                    source_url: Some(format!("https://rera.example/{}", project.registration)),
                    model: None,
                    skill_id: Some("fetch_rera".to_string()),
                    triggered_by: Some("asset_dag".to_string()),
                    learned_at: observed_at,
                    run_id: "rera-fixture".to_string(),
                    input_hash: format!("sha256:{}", project.registration),
                });
                detail_fact_annotations.push(SkillFactAnnotationRecord {
                    entity_id,
                    fact_key: "rera_lat_lng".to_string(),
                    display_template: Some("RERA coordinates: {value}".to_string()),
                    answers_preferences_json: "[]".to_string(),
                    scoring_direction: None,
                    scoring_weight: None,
                    scoring_thresholds_json: "[]".to_string(),
                });
            }
            ReraProjectSnapshotRecord {
                ack_number: None,
                registration_number: Some(project.registration.to_string()),
                project_name: project.name.to_string(),
                promoter_name: Some("PRESTIGE ESTATES PROJECTS LIMITED".to_string()),
                status: Some(project.status.to_string()),
                project_type: Some("Residential".to_string()),
                project_address: Some("Bengaluru".to_string()),
                area_name: Some("Whitefield".to_string()),
                district: Some("Bengaluru Urban".to_string()),
                taluk: None,
                total_land_area_sqm: Some(project.acres * 4_046.856_422_4),
                land_litigation: Some(false),
                source_url: format!("https://rera.example/{}", project.registration),
                fetched_at: observed_at,
            }
        })
        .collect();
    let external_listing_records = projects
        .iter()
        .map(|project| ExternalListingObservationRecord {
            entity_id: canonical_id(project.registration),
            project_key: Some(project.registration.to_string()),
            source_name: "MagicBricks".to_string(),
            source_url: Some(format!("https://listings.example/{}", slug(project.name))),
            price: Some(project.price),
            price_min: Some(project.price),
            price_max: Some(project.price),
            area_sqft: Some(2_000.0),
            area_sqft_min: Some(2_000.0),
            area_sqft_max: Some(2_000.0),
            price_per_sqft_min: Some(project.price / 2_000.0),
            price_per_sqft_max: Some(project.price / 2_000.0),
            price_display: None,
            area_display: None,
            price_per_sqft_display: None,
            configuration: Some("3BHK".to_string()),
            area_type: Some("super_builtup".to_string()),
            bhk: Some(3.0),
            bathrooms: Some(3.0),
            floor: Some("12".to_string()),
            society: Some(project.name.to_string()),
            locality: Some("Whitefield".to_string()),
            observed_at,
        })
        .collect();
    let watermark = vec![SourceWatermark {
        source: "fixture".to_string(),
        high_watermark: observed_at.to_rfc3339(),
    }];
    let first_project = &projects[0];

    AssetSourceInputs {
        rera_registry_monthly: Some(ReraRegistryMonthlyInput {
            snapshot_date: "2026-07".to_string(),
            projects: rera_projects,
            detail_facts,
            detail_fact_annotations,
            source_watermarks: watermark.clone(),
        }),
        google_places_weekly: Some(GooglePlacesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: vec![GooglePlaceSnapshotRecord {
                entity_id: canonical_id(first_project.registration),
                project_key: Some(first_project.registration.to_string()),
                query: "Prestige Raintree Park Whitefield".to_string(),
                place_name: Some("Prestige Raintree Park".to_string()),
                place_id: None,
                reviews_url: "https://maps.example/raintree/reviews".to_string(),
                rating: None,
                review_count: None,
                review_snippets: Vec::new(),
                address: Some("Whitefield".to_string()),
                latitude: None,
                longitude: None,
                confidence: 0.7,
                fetched_at: observed_at,
                fetch_source: "fixture".to_string(),
            }],
            source_watermarks: watermark.clone(),
        }),
        google_nearby_places_weekly: Some(GoogleNearbyPlacesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: vec![GoogleNearbyPlaceRecord {
                entity_id: canonical_id(first_project.registration),
                project_key: Some(first_project.registration.to_string()),
                query: "schools near Prestige Raintree Park".to_string(),
                category: "school".to_string(),
                place_name: "Greenwood High".to_string(),
                place_id: Some("greenwood-high".to_string()),
                place_url: "https://maps.example/greenwood-high".to_string(),
                distance_km: Some(1.2),
                latitude: Some(12.9720),
                longitude: Some(77.5960),
                rating: Some(4.3),
                review_count: Some(420),
                primary_type: Some("school".to_string()),
                place_types: vec!["school".to_string()],
                confidence: 0.82,
                fetched_at: observed_at,
                fetch_source: "fixture".to_string(),
            }],
            source_watermarks: watermark.clone(),
        }),
        external_listings_weekly: Some(ExternalListingsWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: external_listing_records,
            source_watermarks: watermark.clone(),
        }),
        external_images_weekly: Some(ExternalImagesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: projects
                .iter()
                .map(|project| ExternalImageObservationRecord {
                    entity_id: canonical_id(project.registration),
                    project_key: Some(project.registration.to_string()),
                    source_name: "MagicBricks".to_string(),
                    source_page_url: format!("https://images.example/{}", slug(project.name)),
                    image_url: format!("https://images.example/{}/hero.jpg", slug(project.name)),
                    image_kind: Some("exterior".to_string()),
                    width: Some(1200),
                    height: Some(800),
                    rank: Some(1),
                    score: Some(90.0),
                    alt_text: Some(format!("{} exterior", project.name)),
                    storage_policy: Some("link_only".to_string()),
                    content_sha256: None,
                    observed_at,
                })
                .collect(),
            source_watermarks: watermark.clone(),
        }),
        ..AssetSourceInputs::default()
    }
}

fn canonical_id(project_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_key.as_bytes());
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("society:rera-{}", &digest[..16])
}

fn asset_id(value: &str) -> backend::assets::AssetId {
    backend::assets::AssetId::new(value).unwrap()
}

fn slug(value: &str) -> String {
    value
        .to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
