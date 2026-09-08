use std::collections::HashSet;
use std::path::PathBuf;

use backend::assets::MaterializationId;
use backend::data_loader::runtime_snapshot_from_serving_bundle;
use backend::lake::LakeStore;
use backend::search::{GeographyMatchKind, SearchEngine};
use backend::serving::{ServingBundleLoader, ServingEntityVisibility};

#[tokio::test]
#[ignore = "requires an isolated Issue 118 format-12 lake and materialization"]
async fn isolated_geo_cell_materialization_survives_parquet_runtime_and_api_projection() {
    let lake_root = std::env::var_os("OPENESTATES_ISSUE118_LAKE_ROOT")
        .expect("OPENESTATES_ISSUE118_LAKE_ROOT is required for the ignored live contract");
    let materialization_id = std::env::var_os("OPENESTATES_ISSUE118_MATERIALIZATION_ID").expect(
        "OPENESTATES_ISSUE118_MATERIALIZATION_ID is required for the ignored live contract",
    );
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
    assert_eq!(bundle.manifest.format_version, 12);
    let snapshot_identity = bundle.manifest.bundle_version.clone();

    let internal_cells = bundle
        .entities
        .iter()
        .filter(|entity| entity.visibility == ServingEntityVisibility::Internal)
        .map(|entity| (entity.entity_id.clone(), entity.name.clone()))
        .collect::<Vec<_>>();
    assert_eq!(internal_cells.len(), 370);
    let internal_ids = internal_cells
        .iter()
        .map(|(entity_id, _)| entity_id.as_str())
        .collect::<HashSet<_>>();
    assert!(bundle
        .entity_alias_index
        .records()
        .all(|alias| !internal_ids.contains(alias.entity_id.as_str())));
    for (entity_id, name) in &internal_cells {
        assert!(bundle
            .recall_index
            .search(name, 20)
            .expect("internal-name recall query remains valid")
            .iter()
            .all(|hit| hit.entity_id != *entity_id));
    }

    let snapshot = runtime_snapshot_from_serving_bundle(std::sync::Arc::new(bundle));
    let search = |query| SearchEngine::new(&snapshot).search(query);

    let united = search("Godrej United");
    assert_eq!(
        united
            .results
            .iter()
            .take(5)
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        [
            "discovered-godrej-united-2bhk",
            "discovered-godrej-united-3bhk",
            "discovered-godrej-united-4bhk",
            "discovered-godrej-united-5bhk",
            "discovered-sumadhura-nandanam-2bhk",
        ]
    );
    assert!(united.results[..4].iter().all(|result| {
        result
            .geography_match
            .as_ref()
            .is_some_and(|matched| matched.kind == GeographyMatchKind::ExactSociety)
    }));
    assert_eq!(
        united.results[0]
            .geography_match
            .as_ref()
            .and_then(|matched| matched.cell_path.first())
            .map(String::as_str),
        Some("area:osm:relation-19883399")
    );

    let nandanam = search("Sumadhura Nandanam 2BHK under 2Cr");
    assert_eq!(
        nandanam
            .results
            .first()
            .map(|result| result.card.id.as_str()),
        Some("discovered-sumadhura-nandanam-2bhk")
    );
    assert_eq!(
        nandanam.results[0]
            .geography_match
            .as_ref()
            .and_then(|matched| matched.cell_path.first())
            .map(String::as_str),
        Some("area:osm:relation-19883399")
    );

    let green_gables = search("Prestige Green Gables 3BHK under 4Cr");
    assert_eq!(
        green_gables
            .results
            .iter()
            .map(|result| result.card.id.as_str())
            .collect::<Vec<_>>(),
        [
            "discovered-prestige-green-gables-3bhk",
            "discovered-the-residences-at-brigade-tech-gardens-3bhk",
        ]
    );
    assert_eq!(
        green_gables.results[0]
            .geography_match
            .as_ref()
            .and_then(|matched| matched.cell_path.first())
            .map(String::as_str),
        Some("area:osm:relation-19883259")
    );
    assert!(green_gables.results[1]
        .geography_match
        .as_ref()
        .is_some_and(|matched| matched.kind == GeographyMatchKind::CellNearby));

    for result in united.results.iter().chain(green_gables.results.iter()) {
        let matched = result
            .geography_match
            .as_ref()
            .expect("scoped live result has geography metadata");
        assert!(!matched.evidence_refs.is_empty());
        for evidence in &matched.evidence_refs {
            evidence
                .validate_for(&evidence.subject_entity_id, &snapshot_identity)
                .expect("API geography evidence retains immutable identity");
        }
    }

    let chained = search("Godrej United 3BHK or Prestige Green Gables 3BHK under 4Cr");
    assert_eq!(chained.result_sets.len(), 2);
    assert_eq!(chained.result_sets[0].branch_id, "branch-1");
    assert_eq!(
        chained.result_sets[0].results[0].card.id,
        "discovered-godrej-united-3bhk"
    );
    assert_eq!(chained.result_sets[1].branch_id, "branch-2");
    assert_eq!(
        chained.result_sets[1].results[0].card.id,
        "discovered-prestige-green-gables-3bhk"
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
