use backend::assets::{
    rera_legal_facts_input, AssetPartition, CanonicalSocietyMaterializer, MaterializationId,
    ReraProjectSnapshotRecord, ReraRegistryMaterializer, ReraRegistryMonthlyInput,
    SkillFactAnnotationRecord, SkillFactRecord,
};
use backend::knowledge::FactValue;
use backend::lake::LakeStore;
use chrono::{TimeZone, Utc};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[tokio::test]
async fn detailed_rera_facts_round_trip_through_raw_parquet_and_override_listing_facts() {
    let root = tempdir().unwrap();
    let lake = LakeStore::local(root.path()).unwrap();
    let listing_time = Utc.with_ymd_and_hms(2026, 7, 10, 8, 0, 0).unwrap();
    let detail_time = Utc.with_ymd_and_hms(2026, 7, 14, 9, 31, 0).unwrap();
    let project_key = "PRM-DETAIL-1";
    let entity_id = canonical_entity_id(project_key);
    let detail_facts = vec![
        fact(
            &entity_id,
            "rera_status",
            FactValue::Text("Under Construction".to_string()),
            detail_time,
        ),
        fact(
            &entity_id,
            "rera_total_land_area_sqm",
            FactValue::Numeric(48_562.3),
            detail_time,
        ),
    ];
    let detail_fact_annotations = vec![
        annotation(&entity_id, "rera_status", "RERA Status: {value}"),
        annotation(
            &entity_id,
            "rera_total_land_area_sqm",
            "Total Land Area: {value} sq m",
        ),
    ];
    let run_id = MaterializationId::new();
    let rera = ReraRegistryMaterializer::new(lake.clone())
        .materialize_for_run(
            &ReraRegistryMonthlyInput {
                snapshot_date: "2026-07".to_string(),
                projects: vec![ReraProjectSnapshotRecord {
                    ack_number: Some("ACK-DETAIL-1".to_string()),
                    registration_number: Some(project_key.to_string()),
                    project_name: "Detail Proof".to_string(),
                    promoter_name: Some("Proof Builder".to_string()),
                    status: Some("Approved listing value".to_string()),
                    project_type: None,
                    project_address: None,
                    area_name: Some("Whitefield".to_string()),
                    district: None,
                    taluk: None,
                    total_land_area_sqm: None,
                    land_litigation: None,
                    source_url: "https://rera.example/listing".to_string(),
                    fetched_at: listing_time,
                }],
                detail_facts,
                detail_fact_annotations,
                source_watermarks: Vec::new(),
            },
            run_id.clone(),
            AssetPartition::global(),
        )
        .await
        .unwrap();

    assert!(rera
        .artifacts
        .iter()
        .any(|artifact| artifact.key.ends_with("detail_facts/part-00000.parquet")));
    assert!(rera.artifacts.iter().any(|artifact| artifact
        .key
        .ends_with("detail_fact_annotations/part-00000.parquet")));

    let canonical = CanonicalSocietyMaterializer::new(lake.clone())
        .materialize_from_rera_for_run(
            &rera,
            "2026-07-14",
            run_id.clone(),
            AssetPartition::global(),
        )
        .await
        .unwrap();
    let input = rera_legal_facts_input(&lake, &rera, &canonical, &run_id)
        .await
        .unwrap();

    let status = input
        .facts
        .iter()
        .find(|fact| fact.entity_id == entity_id && fact.fact_key == "rera_status")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<FactValue>(&status.value_json).unwrap(),
        FactValue::Text("Under Construction".to_string())
    );
    assert_eq!(
        status.source_url.as_deref(),
        Some("https://rera.example/detail")
    );
    assert_eq!(status.learned_at, detail_time);

    let acreage = input
        .facts
        .iter()
        .find(|fact| fact.entity_id == entity_id && fact.fact_key == "rera_total_land_area_sqm")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<FactValue>(&acreage.value_json).unwrap(),
        FactValue::Numeric(48_562.3)
    );
    assert!(input.fact_annotations.iter().any(|annotation| {
        annotation.entity_id == entity_id
            && annotation.fact_key == "rera_total_land_area_sqm"
            && annotation.display_template.as_deref() == Some("Total Land Area: {value} sq m")
    }));
}

fn fact(
    entity_id: &str,
    fact_key: &str,
    value: FactValue,
    learned_at: chrono::DateTime<Utc>,
) -> SkillFactRecord {
    let value_type = match &value {
        FactValue::Numeric(_) => "numeric",
        FactValue::Text(_) => "text",
        _ => unreachable!(),
    };
    SkillFactRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        value_type: value_type.to_string(),
        value_json: serde_json::to_string(&value).unwrap(),
        confidence: 1.0,
        source_type: "Rera".to_string(),
        source_url: Some("https://rera.example/detail".to_string()),
        model: None,
        skill_id: Some("fetch_rera".to_string()),
        triggered_by: Some("asset_dag".to_string()),
        learned_at,
        run_id: "collector-fetch_rera-2026-07-14".to_string(),
        input_hash: format!("sha256:{fact_key}"),
    }
}

fn annotation(
    entity_id: &str,
    fact_key: &str,
    display_template: &str,
) -> SkillFactAnnotationRecord {
    SkillFactAnnotationRecord {
        entity_id: entity_id.to_string(),
        fact_key: fact_key.to_string(),
        display_template: Some(display_template.to_string()),
        answers_preferences_json: "[]".to_string(),
        scoring_direction: None,
        scoring_weight: None,
        scoring_thresholds_json: "[]".to_string(),
    }
}

fn canonical_entity_id(project_key: &str) -> String {
    let digest = Sha256::digest(project_key.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("society:rera-{}", &hex[..16])
}
