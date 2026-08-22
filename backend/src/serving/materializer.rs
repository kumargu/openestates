use std::fmt;

use crate::assets::{
    ArtifactRef, AssetId, AssetMaterializationStore, AssetPartition, AssetStage,
    KgSocietyViewMaterialization, KgViewRecords, MaterializationId, MaterializationRecord,
    SourceWatermark, KG_SOCIETY_VIEW_ASSET_ID,
};
use crate::dag_config::ServingAdmissionProfile;
use crate::knowledge::KnowledgeGraph;
use crate::lake::{LakeError, LakeStore};

use super::{
    ServingBundleBuilder, ServingBundleError, ServingBundleManifest, ServingEdgeRecord,
    ServingEntityRecord, ServingFactRecord, ServingReraEvidenceRecord, ServingSearchMetadataRecord,
    SEARCH_SERVING_BUNDLE_ASSET_ID,
};

#[derive(Clone)]
pub struct SearchServingBundleMaterializer {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
    admission_profile: ServingAdmissionProfile,
}

#[derive(Debug, Clone)]
pub struct SearchServingBundleMaterialization {
    pub manifest: ServingBundleManifest,
    pub record: MaterializationRecord,
}

impl SearchServingBundleMaterializer {
    pub fn new(lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
            admission_profile: ServingAdmissionProfile::BuyerCatalog,
        }
    }

    pub fn for_search_experiment(lake: LakeStore) -> Self {
        let materializations = AssetMaterializationStore::new(lake.clone());
        Self {
            lake,
            materializations,
            admission_profile: ServingAdmissionProfile::SearchExperiment,
        }
    }

    fn bundle_builder(&self) -> ServingBundleBuilder {
        match self.admission_profile {
            ServingAdmissionProfile::BuyerCatalog => ServingBundleBuilder::new(self.lake.clone()),
            ServingAdmissionProfile::SearchExperiment => {
                ServingBundleBuilder::for_search_experiment(self.lake.clone())
            }
        }
    }

    fn ensure_promotion_allowed(&self) -> Result<(), SearchServingBundleMaterializeError> {
        if self.admission_profile == ServingAdmissionProfile::SearchExperiment {
            return Err(SearchServingBundleMaterializeError::ExperimentPromotionForbidden);
        }
        Ok(())
    }

    pub async fn materialize_and_promote(
        &self,
        graph: &KnowledgeGraph,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        let records = KgViewRecords::from_graph(graph)?;
        self.materialize_and_promote_with_parents(
            &records,
            bundle_version,
            source_watermarks,
            Vec::new(),
        )
        .await
    }

    pub async fn materialize_and_promote_from_kg_view(
        &self,
        kg_view: &KgSocietyViewMaterialization,
        bundle_version: impl Into<String>,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.materialize_and_promote_from_kg_view_for_run(
            kg_view,
            bundle_version,
            MaterializationId::new(),
            AssetPartition::global(),
        )
        .await
    }

    pub async fn materialize_and_promote_from_kg_view_for_run(
        &self,
        kg_view: &KgSocietyViewMaterialization,
        bundle_version: impl Into<String>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.ensure_promotion_allowed()?;
        let materialization = self
            .materialize_from_kg_view_for_run(kg_view, bundle_version, run_id, partition)
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_from_kg_view_for_run(
        &self,
        kg_view: &KgSocietyViewMaterialization,
        bundle_version: impl Into<String>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.materialize_with_parents_for_run(
            &kg_view.records,
            bundle_version,
            vec![SourceWatermark {
                source: KG_SOCIETY_VIEW_ASSET_ID.to_string(),
                high_watermark: kg_view.record.materialization_id.to_string(),
            }],
            vec![kg_view.record.materialization_id.clone()],
            run_id,
            partition,
        )
        .await
    }

    pub async fn materialize_from_kg_view_and_rera_for_run(
        &self,
        kg_view: &KgSocietyViewMaterialization,
        rera_evidence: Vec<ServingReraEvidenceRecord>,
        rera_parent_materializations: Vec<MaterializationId>,
        bundle_version: impl Into<String>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        let mut parent_materializations = vec![kg_view.record.materialization_id.clone()];
        parent_materializations.extend(rera_parent_materializations);
        let manifest = self
            .bundle_builder()
            .build_from_kg_view_records_with_rera(&kg_view.records, rera_evidence, bundle_version)
            .await?;
        self.write_unpromoted_record(
            manifest,
            vec![SourceWatermark {
                source: KG_SOCIETY_VIEW_ASSET_ID.to_string(),
                high_watermark: kg_view.record.materialization_id.to_string(),
            }],
            parent_materializations,
            run_id,
            partition,
        )
        .await
    }

    pub async fn materialize_and_promote_with_parents(
        &self,
        records: &KgViewRecords,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.ensure_promotion_allowed()?;
        let materialization = self
            .materialize_with_parents_for_run(
                records,
                bundle_version,
                source_watermarks,
                parent_materializations,
                MaterializationId::new(),
                AssetPartition::global(),
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_and_promote_with_parents_for_run(
        &self,
        records: &KgViewRecords,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.ensure_promotion_allowed()?;
        let materialization = self
            .materialize_with_parents_for_run(
                records,
                bundle_version,
                source_watermarks,
                parent_materializations,
                run_id,
                partition,
            )
            .await?;
        self.materializations
            .promote_current(&materialization.record)
            .await?;
        Ok(materialization)
    }

    pub async fn materialize_with_parents_for_run(
        &self,
        records: &KgViewRecords,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.materialize_with_parents_for_run_inner(
            records,
            bundle_version,
            source_watermarks,
            parent_materializations,
            run_id,
            partition,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_child_from_serving_records_for_run(
        &self,
        entities: Vec<ServingEntityRecord>,
        facts: Vec<ServingFactRecord>,
        search_metadata: Vec<ServingSearchMetadataRecord>,
        edges: Vec<ServingEdgeRecord>,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        self.materialize_child_from_serving_records_with_rera_for_run(
            entities,
            facts,
            search_metadata,
            edges,
            Vec::new(),
            Vec::new(),
            bundle_version,
            source_watermarks,
            parent_materializations,
            run_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize_child_from_serving_records_with_rera_for_run(
        &self,
        entities: Vec<ServingEntityRecord>,
        facts: Vec<ServingFactRecord>,
        search_metadata: Vec<ServingSearchMetadataRecord>,
        edges: Vec<ServingEdgeRecord>,
        rera_evidence: Vec<ServingReraEvidenceRecord>,
        excluded_rera_evidence_society_ids: Vec<String>,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        let manifest = self
            .bundle_builder()
            .build_child_from_serving_records_with_rera(
                entities,
                facts,
                search_metadata,
                edges,
                rera_evidence,
                excluded_rera_evidence_society_ids,
                bundle_version,
            )
            .await?;
        self.write_unpromoted_record(
            manifest,
            source_watermarks,
            parent_materializations,
            run_id,
            AssetPartition::global(),
        )
        .await
    }

    async fn materialize_with_parents_for_run_inner(
        &self,
        records: &KgViewRecords,
        bundle_version: impl Into<String>,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        let manifest = self
            .bundle_builder()
            .build_from_kg_view_records(records, bundle_version)
            .await?;
        self.write_unpromoted_record(
            manifest,
            source_watermarks,
            parent_materializations,
            run_id,
            partition,
        )
        .await
    }

    async fn write_unpromoted_record(
        &self,
        manifest: ServingBundleManifest,
        source_watermarks: Vec<SourceWatermark>,
        parent_materializations: Vec<MaterializationId>,
        run_id: MaterializationId,
        partition: AssetPartition,
    ) -> Result<SearchServingBundleMaterialization, SearchServingBundleMaterializeError> {
        let manifest_key = crate::assets::AssetPathBuilder::serving_bundle_key(
            &manifest.bundle_version,
            "manifest.json",
        );
        let manifest_meta = self.lake.put_json(&manifest_key, &manifest).await?;

        let record = MaterializationRecord::succeeded(
            AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID)
                .expect("static search serving bundle asset id is valid"),
            AssetStage::Serving,
            partition,
            manifest.bundle_version.clone(),
            vec![ArtifactRef::json(manifest_meta)],
        )
        .with_run_id(run_id)
        .with_parent_materializations(parent_materializations)
        .with_source_watermarks(source_watermarks)
        .with_row_count(manifest.entity_count);

        self.materializations.write_materialization(&record).await?;

        Ok(SearchServingBundleMaterialization { manifest, record })
    }
}

#[derive(Debug)]
pub enum SearchServingBundleMaterializeError {
    Bundle(ServingBundleError),
    ExperimentPromotionForbidden,
    Json(serde_json::Error),
    Lake(LakeError),
}

impl fmt::Display for SearchServingBundleMaterializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bundle(err) => write!(f, "search serving bundle build failed: {err}"),
            Self::ExperimentPromotionForbidden => write!(
                f,
                "search-experiment bundles must remain unpromoted and be selected by immutable materialization id"
            ),
            Self::Json(err) => write!(f, "search serving bundle KG conversion failed: {err}"),
            Self::Lake(err) => write!(f, "search serving bundle materialization failed: {err}"),
        }
    }
}

impl std::error::Error for SearchServingBundleMaterializeError {}

impl From<ServingBundleError> for SearchServingBundleMaterializeError {
    fn from(err: ServingBundleError) -> Self {
        Self::Bundle(err)
    }
}

impl From<LakeError> for SearchServingBundleMaterializeError {
    fn from(err: LakeError) -> Self {
        Self::Lake(err)
    }
}

impl From<serde_json::Error> for SearchServingBundleMaterializeError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_experiment_materializer_rejects_current_pointer_promotion() {
        let root = tempfile::tempdir().expect("temporary lake");
        let lake = LakeStore::local(root.path()).expect("local lake");
        let materializer = SearchServingBundleMaterializer::for_search_experiment(lake);

        assert!(matches!(
            materializer.ensure_promotion_allowed(),
            Err(SearchServingBundleMaterializeError::ExperimentPromotionForbidden)
        ));
    }
}
