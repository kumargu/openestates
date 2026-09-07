use std::path::PathBuf;

use backend::assets::MaterializationId;
use backend::data_loader::runtime_snapshot_from_serving_bundle;
use backend::lake::LakeStore;
use backend::search::{GeographyMatchKind, SearchEngine};
use backend::serving::ServingBundleLoader;

const EXPECTED_VERSION: &str = "issue-118-whitefield-115-plus-27-geo-cells-2026-09-07-r11";

#[tokio::test]
async fn isolated_geo_cell_materialization_survives_parquet_runtime_and_api_projection() {
    let Some(lake_root) = std::env::var_os("OPENESTATES_ISSUE118_LAKE_ROOT") else {
        return;
    };
    let Some(materialization_id) = std::env::var_os("OPENESTATES_ISSUE118_MATERIALIZATION_ID")
    else {
        return;
    };
    let materialization_id = materialization_id
        .into_string()
        .expect("materialization id is UTF-8")
        .parse::<MaterializationId>()
        .expect("materialization id is a UUID");
    let lake = LakeStore::local(PathBuf::from(lake_root)).unwrap();
    let cache = tempfile::tempdir().unwrap();
    let bundle = ServingBundleLoader::new(lake, cache.path())
        .load_search_bundle_by_materialization(&materialization_id)
        .await
        .unwrap()
        .expect("isolated Issue 118 materialization exists");
    assert_eq!(bundle.manifest.bundle_version, EXPECTED_VERSION);
    let snapshot = runtime_snapshot_from_serving_bundle(std::sync::Arc::new(bundle));
    let search = |query| SearchEngine::new(&snapshot).search(query);

    let air = search("Godrej Air");
    assert_eq!(
        air.results
            .iter()
            .take(5)
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        [
            "discovered-godrej-air-2bhk",
            "discovered-godrej-air-3bhk",
            "discovered-sumadhura-capitol-residences-3bhk",
            "discovered-sumadhura-nandanam-2bhk",
            "discovered-sumadhura-capitol-residences-4bhk",
        ]
    );
    assert!(air.results[..2].iter().all(|result| {
        result
            .geography_match
            .as_ref()
            .is_some_and(|matched| matched.kind == GeographyMatchKind::ExactSociety)
    }));
    assert!(air.results[2..].iter().all(|result| {
        result
            .geography_match
            .as_ref()
            .is_some_and(|matched| matched.kind == GeographyMatchKind::SameMarketLocality)
    }));

    let budget = search("Godrej Air 2BHK under 2Cr");
    assert_eq!(
        budget.results.first().map(|result| result.card.id.as_str()),
        Some("discovered-sumadhura-nandanam-2bhk")
    );
    assert!(budget
        .results
        .iter()
        .all(|result| result.card.id != "discovered-godrej-air-2bhk"));

    let nearby = search("SNN Clermont");
    assert_eq!(
        nearby
            .results
            .iter()
            .take(4)
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        [
            "discovered-snn-clermont-3bhk",
            "discovered-snn-clermont-4bhk",
            "discovered-century-ethos-1bhk",
            "discovered-century-ethos-3bhk",
        ]
    );
    for result in &nearby.results {
        let matched = result
            .geography_match
            .as_ref()
            .expect("scoped live result has geography metadata");
        assert!(matched.hops <= 2);
        assert!(matched.distance_km.is_some_and(|distance| distance <= 4.0));
        assert!(!matched.cell_path.is_empty());
        assert!(!matched.evidence_refs.is_empty());
        for evidence in &matched.evidence_refs {
            evidence
                .validate_for(&evidence.subject_entity_id, EXPECTED_VERSION)
                .expect("API geography evidence retains immutable identity");
        }
    }

    let chained = search("Godrej Air 2BHK or Prestige Waterford 3BHK");
    assert_eq!(chained.result_sets.len(), 2);
    assert_eq!(chained.result_sets[0].branch_id, "branch-1");
    assert_eq!(
        chained.result_sets[0].results[0].card.id,
        "discovered-godrej-air-2bhk"
    );
    assert_eq!(chained.result_sets[1].branch_id, "branch-2");
    assert_eq!(
        chained.result_sets[1].results[0].card.id,
        "discovered-prestige-waterford-3bhk"
    );
    let api = serde_json::to_value(&chained.result_sets).unwrap();
    assert_eq!(
        api[0]["results"][0]["geographyMatch"]["kind"],
        "exact_society"
    );
    assert!(api[0]["results"][0]["geographyMatch"]["cellPath"]
        .as_array()
        .is_some_and(|path| !path.is_empty()));
}
