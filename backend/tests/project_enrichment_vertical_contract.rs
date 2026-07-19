use backend::assets::{
    default_openestates_registry, read_skill_fact_artifact_rows, AssetDagExecutionOptions,
    AssetDagExecutor, AssetMaterializationStore, AssetPartition, AssetSourceInputs,
    ExternalListingObservationRecord, ExternalListingsWeeklyInput, GooglePlaceSnapshotRecord,
    GooglePlacesWeeklyInput, MetroStationObservationRecord, MetroStationsMonthlyInput,
    PrestigeInventoryObservationRecord, PrestigeInventoryWeeklyInput, RedditThreadSnapshotRecord,
    RedditThreadsDailyInput, ReraProjectSnapshotRecord, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactRecord, SkillFactsInput, SourceWatermark,
    BUILDER_RERA_AGGREGATES_ASSET_ID, EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
    EXTERNAL_LISTING_FACTS_ASSET_ID, MARKET_PROJECT_FACTS_ASSET_ID, METRO_PROXIMITY_FACTS_ASSET_ID,
    METRO_STATIONS_MONTHLY_ASSET_ID, PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
};
use backend::knowledge::{FactValue, KnowledgeGraph};
use backend::lake::LakeStore;
use backend::serving::ServingBundleLoader;
use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[tokio::test]
async fn three_societies_reach_serving_with_market_metro_and_builder_evidence() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let store = AssetMaterializationStore::new(lake.clone());
    let observed_at = Utc.with_ymd_and_hms(2026, 7, 14, 12, 0, 0).unwrap();
    let projects = fixtures(observed_at);
    let source_inputs = source_inputs(&projects, observed_at);
    let partition =
        AssetPartition::new([("dt", "2026-07-14"), ("subreddit", "BangaloreRealEstates")]);

    let report = AssetDagExecutor::new(default_openestates_registry(), lake.clone())
        .execute(
            &KnowledgeGraph::new(),
            AssetDagExecutionOptions::new(partition, observed_at)
                .with_version("2026-07-14T12:00Z")
                .with_source_inputs(source_inputs),
        )
        .await
        .unwrap();

    for asset_id in [
        PRESTIGE_INVENTORY_WEEKLY_ASSET_ID,
        MARKET_PROJECT_FACTS_ASSET_ID,
        EXTERNAL_LISTINGS_WEEKLY_ASSET_ID,
        EXTERNAL_LISTING_FACTS_ASSET_ID,
        METRO_STATIONS_MONTHLY_ASSET_ID,
        METRO_PROXIMITY_FACTS_ASSET_ID,
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

    let prestige_record = store
        .current_record(
            &asset_id(PRESTIGE_INVENTORY_WEEKLY_ASSET_ID),
            &AssetPartition::new([("source", "prestige")]),
        )
        .await
        .unwrap();
    assert_eq!(prestige_record.row_count, 3);
    assert!(prestige_record.artifacts.iter().any(|artifact| {
        artifact.content_type == "application/vnd.apache.parquet"
            && artifact.key.ends_with("projects/part-00000.parquet")
    }));

    let metro_record = store
        .current_record(
            &asset_id(METRO_STATIONS_MONTHLY_ASSET_ID),
            &AssetPartition::new([("source", "openstreetmap")]),
        )
        .await
        .unwrap();
    assert_eq!(metro_record.row_count, 2);
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
    assert!(
        builder_asset_rows
            .facts
            .iter()
            .any(|fact| fact.entity_id == "builder:prestige-estates-projects-limited"),
        "builder asset entities: {:?}",
        builder_asset_rows
            .facts
            .iter()
            .map(|fact| fact.entity_id.as_str())
            .collect::<Vec<_>>()
    );

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
            "market_starting_price_inr",
            "market_bhk_options",
            "listing_3bhk",
            "listing_price_3bhk",
            "listing_area_sqft_3bhk",
            "official_project_url",
            "nearest_operational_metro_station",
            "metro_distance_km",
        ] {
            assert!(
                rows.facts.iter().any(|fact| fact.fact_key == fact_key),
                "{alias} should contain {fact_key}"
            );
        }
        let market_fact = rows
            .facts
            .iter()
            .find(|fact| fact.fact_key == "market_starting_price_inr")
            .unwrap();
        assert_eq!(market_fact.source_type, "BuilderOfficial");
        assert!(market_fact.source_url.is_some());
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
    source_project_id: &'static str,
    source_slug: &'static str,
    status: &'static str,
    acres: f64,
    price: f64,
    units: u64,
    latitude: f64,
    longitude: f64,
}

fn fixtures(_observed_at: chrono::DateTime<Utc>) -> Vec<ProjectFixture> {
    vec![
        ProjectFixture {
            name: "Prestige Raintree Park",
            registration: "PRM/KA/RERA/1251/446/PR/270824/006981",
            source_project_id: "2201",
            source_slug: "prestige-raintree-park",
            status: "Under Construction",
            acres: 21.0,
            price: 37_000_000.0,
            units: 1520,
            latitude: 12.95384,
            longitude: 77.74546,
        },
        ProjectFixture {
            name: "Prestige Park Grove",
            registration: "PRM/KA/RERA/1251/446/PR/100823/006141",
            source_project_id: "2212",
            source_slug: "prestige-park-grove",
            status: "Sold Out",
            acres: 71.41,
            price: 36_000_000.0,
            units: 3713,
            latitude: 13.01903,
            longitude: 77.75745,
        },
        ProjectFixture {
            name: "Prestige Lavender Fields",
            registration: "PRM/KA/RERA/1251/446/PR/290423/005906",
            source_project_id: "2199",
            source_slug: "prestige-lavender-fields",
            status: "Sold Out",
            acres: 18.2,
            price: 24_300_000.0,
            units: 1473,
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
    let prestige_records = projects
        .iter()
        .map(|project| PrestigeInventoryObservationRecord {
            entity_id: canonical_id(project.registration),
            project_key: Some(project.registration.to_string()),
            source_project_id: project.source_project_id.to_string(),
            source_project_name: project.name.to_string(),
            source_project_slug: project.source_slug.to_string(),
            source_url: format!(
                "https://www.prestigeconstructions.com/residential-projects/bangalore/{}",
                project.source_slug
            ),
            status: Some(project.status.to_string()),
            land_area_acres: Some(project.acres),
            starting_price_inr: Some(project.price),
            price_display: Some(format!("INR {} onwards", project.price)),
            bhk_options: vec!["2".to_string(), "3".to_string(), "4".to_string()],
            total_units: Some(project.units),
            latitude: Some(project.latitude),
            longitude: Some(project.longitude),
            maps_url: Some(format!("https://maps.example/{}", project.source_slug)),
            address: Some("Whitefield, Bengaluru".to_string()),
            observed_at,
        })
        .collect();
    let external_listing_records = projects
        .iter()
        .map(|project| ExternalListingObservationRecord {
            entity_id: canonical_id(project.registration),
            project_key: Some(project.registration.to_string()),
            source_name: "FixtureListings".to_string(),
            source_url: Some(format!("https://listings.example/{}", project.source_slug)),
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

    AssetSourceInputs {
        rera_registry_monthly: Some(ReraRegistryMonthlyInput {
            snapshot_date: "2026-07".to_string(),
            projects: rera_projects,
            detail_facts,
            detail_fact_annotations,
            source_watermarks: watermark.clone(),
        }),
        reddit_threads_daily: Some(RedditThreadsDailyInput {
            snapshot_date: "2026-07-14".to_string(),
            subreddit: "BangaloreRealEstates".to_string(),
            records: vec![RedditThreadSnapshotRecord {
                thread_id: "fixture-thread".to_string(),
                subreddit: "BangaloreRealEstates".to_string(),
                query: "Prestige Raintree Park".to_string(),
                title: "Fixture resident thread".to_string(),
                url: Some("https://reddit.example/thread".to_string()),
                score: 1,
                num_comments: 1,
                created_utc: Some(observed_at.timestamp()),
                selftext: Some("Fixture evidence".to_string()),
                fetched_at: observed_at,
                fetch_source: "fixture".to_string(),
            }],
            source_watermarks: watermark.clone(),
        }),
        reddit_resident_facts: Some(one_support_fact(
            "reddit",
            "society:prestige-raintree-park",
            "resident_fixture_signal",
            observed_at,
            &watermark,
        )),
        legacy_seed_facts: Some(SkillFactsInput {
            source: "legacy_seed".to_string(),
            snapshot_date: "2026-07-14".to_string(),
            facts: vec![],
            fact_annotations: vec![],
            source_watermarks: watermark.clone(),
        }),
        google_places_weekly: Some(GooglePlacesWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: vec![GooglePlaceSnapshotRecord {
                entity_id: canonical_id(projects[0].registration),
                project_key: Some(projects[0].registration.to_string()),
                query: "Prestige Raintree Park Whitefield".to_string(),
                place_name: Some("Prestige Raintree Park".to_string()),
                place_id: None,
                reviews_url: "https://maps.example/raintree/reviews".to_string(),
                rating: None,
                review_count: None,
                review_snippets: Vec::new(),
                address: Some("Whitefield".to_string()),
                confidence: 0.7,
                fetched_at: observed_at,
                fetch_source: "fixture".to_string(),
            }],
            source_watermarks: watermark.clone(),
        }),
        prestige_inventory_weekly: Some(PrestigeInventoryWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: prestige_records,
            source_watermarks: watermark.clone(),
        }),
        external_listings_weekly: Some(ExternalListingsWeeklyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: external_listing_records,
            source_watermarks: watermark.clone(),
        }),
        metro_stations_monthly: Some(MetroStationsMonthlyInput {
            snapshot_date: "2026-07-14".to_string(),
            records: vec![
                MetroStationObservationRecord {
                    station_id: "node:1".to_string(),
                    name: "Hopefarm Channasandra".to_string(),
                    network: Some("Namma Metro".to_string()),
                    operator: Some("BMRCL".to_string()),
                    status: "operational".to_string(),
                    latitude: 12.9873,
                    longitude: 77.7538,
                    source_url: "https://www.openstreetmap.org/node/1".to_string(),
                    observed_at,
                },
                MetroStationObservationRecord {
                    station_id: "node:2".to_string(),
                    name: "Kadugodi Tree Park".to_string(),
                    network: Some("Namma Metro".to_string()),
                    operator: Some("BMRCL".to_string()),
                    status: "operational".to_string(),
                    latitude: 12.9965,
                    longitude: 77.7612,
                    source_url: "https://www.openstreetmap.org/node/2".to_string(),
                    observed_at,
                },
            ],
            source_watermarks: watermark,
        }),
        ..AssetSourceInputs::default()
    }
}

fn one_support_fact(
    source: &str,
    entity_id: &str,
    fact_key: &str,
    learned_at: chrono::DateTime<Utc>,
    watermarks: &[SourceWatermark],
) -> SkillFactsInput {
    SkillFactsInput {
        source: source.to_string(),
        snapshot_date: "2026-07-14".to_string(),
        facts: vec![SkillFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: "text".to_string(),
            value_json: serde_json::to_string(&FactValue::Text("present".to_string())).unwrap(),
            confidence: 0.5,
            source_type: "Reddit".to_string(),
            source_url: Some("https://reddit.example/thread".to_string()),
            model: None,
            skill_id: Some("fixture".to_string()),
            triggered_by: Some("asset_dag".to_string()),
            learned_at,
            run_id: "fixture".to_string(),
            input_hash: "sha256:fixture".to_string(),
        }],
        fact_annotations: vec![SkillFactAnnotationRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            display_template: Some("Fixture: {value}".to_string()),
            answers_preferences_json: "[]".to_string(),
            scoring_direction: None,
            scoring_weight: None,
            scoring_thresholds_json: "[]".to_string(),
        }],
        source_watermarks: watermarks.to_vec(),
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
