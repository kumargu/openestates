use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakeStore};
use crate::serving::{
    read_edges_parquet, read_entities_parquet, read_facts_parquet, read_rera_evidence_parquet,
    read_search_metadata_parquet, unique_society_aliases, validate_search_serving_candidate,
    ServingBundleManifest, ServingFactIndex, SEARCH_SERVING_BUNDLE_ASSET_ID,
};

use super::promotion::ServingReleasePromotionError;
use super::{
    validate_search_serving_convergence, AssetId, AssetMaterializationStore, MaterializationId,
    MaterializationStatus, CURRENT_PROJECT_FACTS_ASSET_ID, KG_SOCIETY_VIEW_ASSET_ID,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CatalogReleaseId(Uuid);

impl CatalogReleaseId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[cfg(test)]
    pub fn fixed(value: impl AsRef<str>) -> Self {
        Self(Uuid::parse_str(value.as_ref()).expect("valid fixed catalog release id"))
    }
}

impl Default for CatalogReleaseId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CatalogReleaseId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl FromStr for CatalogReleaseId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogEnvironment {
    Dev,
    Staging,
    Production,
}

impl fmt::Display for CatalogEnvironment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Production => "production",
        })
    }
}

impl FromStr for CatalogEnvironment {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dev" => Ok(Self::Dev),
            "staging" => Ok(Self::Staging),
            "production" | "prod" => Ok(Self::Production),
            other => Err(format!(
                "unknown catalog environment {other}; expected dev, staging, or production"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogValidationStatus {
    Draft,
    Validated,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogMembershipKind {
    Reused,
    Added,
    Refreshed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogMembership {
    pub society_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_config_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rera_id: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<CatalogCoordinates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_materialization_id: Option<MaterializationId>,
    pub membership_kind: CatalogMembershipKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogCoordinates {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogTombstone {
    pub entity_id: String,
    pub reason: String,
    pub removed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedMaterialization {
    pub asset_id: AssetId,
    pub materialization_id: MaterializationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedCatalogAssets {
    pub serving_materialization_id: MaterializationId,
    pub kg_materialization_id: MaterializationId,
    pub project_facts_materialization_id: MaterializationId,
    #[serde(default)]
    pub materializations: Vec<PinnedMaterialization>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CatalogReleaseChanges {
    #[serde(default)]
    pub added_societies: Vec<String>,
    #[serde(default)]
    pub refreshed_societies: Vec<String>,
    #[serde(default)]
    pub removed_societies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGateStatus {
    Passed,
    Warning,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityGateResult {
    pub gate: String,
    pub status: QualityGateStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityReport {
    pub status: QualityGateStatus,
    #[serde(default)]
    pub gates: Vec<QualityGateResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warm_search_p95_ms: Option<u32>,
    pub generated_at: DateTime<Utc>,
}

impl QualityReport {
    fn passed(gates: Vec<QualityGateResult>) -> Self {
        let status = if gates
            .iter()
            .any(|gate| gate.status == QualityGateStatus::Failed)
        {
            QualityGateStatus::Failed
        } else {
            QualityGateStatus::Passed
        };
        Self {
            status,
            gates,
            warm_search_p95_ms: None,
            generated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogRelease {
    pub release_id: CatalogReleaseId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_release_id: Option<CatalogReleaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub changes: CatalogReleaseChanges,
    #[serde(default)]
    pub pinned_inputs: Vec<PinnedMaterialization>,
    pub derived_assets: DerivedCatalogAssets,
    #[serde(default)]
    pub memberships: Vec<CatalogMembership>,
    #[serde(default)]
    pub tombstones: Vec<CatalogTombstone>,
    pub validation_status: CatalogValidationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_report: Option<QualityReport>,
}

impl CatalogRelease {
    pub fn candidate(
        base_release_id: Option<CatalogReleaseId>,
        description: Option<String>,
        derived_assets: DerivedCatalogAssets,
    ) -> Self {
        Self {
            release_id: CatalogReleaseId::new(),
            base_release_id,
            description,
            created_at: Utc::now(),
            changes: CatalogReleaseChanges::default(),
            pinned_inputs: Vec::new(),
            derived_assets,
            memberships: Vec::new(),
            tombstones: Vec::new(),
            validation_status: CatalogValidationStatus::Draft,
            quality_report: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentPointer {
    pub environment: CatalogEnvironment,
    pub release_id: CatalogReleaseId,
    pub release_key: String,
    pub serving_materialization_id: MaterializationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_release_id: Option<CatalogReleaseId>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct CatalogReleaseStore {
    lake: LakeStore,
    materializations: AssetMaterializationStore,
}

impl CatalogReleaseStore {
    pub fn new(lake: LakeStore) -> Self {
        Self {
            materializations: AssetMaterializationStore::new(lake.clone()),
            lake,
        }
    }

    pub async fn write_release(
        &self,
        release: &CatalogRelease,
    ) -> Result<ArtifactMetadata, LakeError> {
        self.lake
            .put_json(&release_key(&release.release_id), release)
            .await
    }

    pub async fn release(
        &self,
        release_id: &CatalogReleaseId,
    ) -> Result<CatalogRelease, LakeError> {
        let release: CatalogRelease = self.lake.get_json(&release_key(release_id)).await?;
        if release.release_id != *release_id {
            return Err(LakeError::InvalidMetadata(format!(
                "catalog release record at {} has id {}, expected {release_id}",
                release_key(release_id),
                release.release_id
            )));
        }
        Ok(release)
    }

    pub async fn validate_release(
        &self,
        release_id: &CatalogReleaseId,
    ) -> Result<CatalogRelease, CatalogReleaseError> {
        let mut release = self.release(release_id).await?;
        let mut gates = Vec::new();

        gates.extend(self.validate_materialization_lineage(&release).await?);
        gates.extend(self.validate_serving_candidate(&release).await?);
        gates.extend(self.validate_memberships(&release).await?);
        gates.extend(
            self.validate_serving_projection_memberships(&release)
                .await?,
        );
        gates.extend(self.validate_serving_rera_scope(&release).await?);

        let quality_report = QualityReport::passed(gates);
        release.validation_status = if quality_report.status == QualityGateStatus::Passed {
            CatalogValidationStatus::Validated
        } else {
            CatalogValidationStatus::Rejected
        };
        release.quality_report = Some(quality_report);
        self.write_release(&release).await?;
        Ok(release)
    }

    pub async fn current_pointer(
        &self,
        environment: CatalogEnvironment,
    ) -> Result<Option<EnvironmentPointer>, LakeError> {
        match self
            .lake
            .get_json::<EnvironmentPointer>(&environment_pointer_key(environment))
            .await
        {
            Ok(pointer) => {
                if pointer.environment != environment {
                    return Err(LakeError::InvalidMetadata(format!(
                        "catalog environment pointer for {environment} belongs to {}",
                        pointer.environment
                    )));
                }
                Ok(Some(pointer))
            }
            Err(err) if err.is_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub async fn promote_environment(
        &self,
        release_id: &CatalogReleaseId,
        environment: CatalogEnvironment,
        options: PromoteCatalogReleaseOptions,
    ) -> Result<EnvironmentPointer, CatalogReleaseError> {
        let release = self.release(release_id).await?;
        if release.validation_status != CatalogValidationStatus::Validated {
            return Err(CatalogReleaseError::GateRejected(format!(
                "release {release_id} is {:?}; run validate before promotion",
                release.validation_status
            )));
        }
        if release
            .quality_report
            .as_ref()
            .is_some_and(|report| report.status == QualityGateStatus::Failed)
        {
            return Err(CatalogReleaseError::GateRejected(format!(
                "release {release_id} has failing quality gates"
            )));
        }
        if environment == CatalogEnvironment::Production {
            if !options.approve_production {
                return Err(CatalogReleaseError::GateRejected(
                    "production promotion requires --approve-production".to_string(),
                ));
            }
            let staging = self.current_pointer(CatalogEnvironment::Staging).await?;
            if staging.as_ref().map(|pointer| &pointer.release_id) != Some(release_id) {
                return Err(CatalogReleaseError::GateRejected(format!(
                    "release {release_id} must be promoted to staging before production"
                )));
            }
        }
        self.require_eligibility_quarantine_format(&release).await?;
        if environment == CatalogEnvironment::Production {
            let serving_asset =
                AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
            let serving_record = self
                .materializations
                .record_by_id_for_asset(
                    &serving_asset,
                    &release.derived_assets.serving_materialization_id,
                )
                .await?
                .ok_or_else(|| {
                    CatalogReleaseError::MissingMaterialization(
                        release.derived_assets.serving_materialization_id.clone(),
                    )
                })?;
            if options.force_legacy_pointer {
                super::promotion::promote_search_serving_release(
                    &self.materializations,
                    &serving_record,
                    true,
                )
                .await?;
            }
        }

        let key = environment_pointer_key(environment);
        let previous = self.current_pointer(environment).await?;
        let pointer = EnvironmentPointer {
            environment,
            release_id: release.release_id.clone(),
            release_key: release_key(&release.release_id).to_string(),
            serving_materialization_id: release.derived_assets.serving_materialization_id,
            previous_release_id: previous.as_ref().map(|pointer| pointer.release_id.clone()),
            updated_at: Utc::now(),
        };
        let expected = options.expected_current_release.as_ref();
        let promoted = self
            .lake
            .put_json_if(&key, &pointer, |current: Option<&EnvironmentPointer>| {
                current.map(|pointer| &pointer.release_id) == expected
            })
            .await?;
        if !promoted {
            let current = self.current_pointer(environment).await?;
            return Err(CatalogReleaseError::PromotionRejected {
                environment,
                desired: release.release_id,
                current: current.map(|pointer| pointer.release_id),
            });
        }
        Ok(pointer)
    }

    async fn require_eligibility_quarantine_format(
        &self,
        release: &CatalogRelease,
    ) -> Result<(), CatalogReleaseError> {
        let serving_asset =
            AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
        let serving_record = self
            .materializations
            .record_by_id_for_asset(
                &serving_asset,
                &release.derived_assets.serving_materialization_id,
            )
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(
                    release.derived_assets.serving_materialization_id.clone(),
                )
            })?;
        let manifest_artifact = serving_record
            .artifacts
            .iter()
            .find(|artifact| artifact.key.ends_with("manifest.json"))
            .ok_or_else(|| {
                CatalogReleaseError::GateRejected(format!(
                    "serving materialization {} has no manifest artifact",
                    serving_record.materialization_id
                ))
            })?;
        let manifest_key = LakeKey::new(manifest_artifact.key.clone()).map_err(|error| {
            CatalogReleaseError::GateRejected(format!("invalid serving manifest key: {error}"))
        })?;
        let manifest: ServingBundleManifest = self.lake.get_json(&manifest_key).await?;
        if manifest.format_version < 8 {
            return Err(CatalogReleaseError::GateRejected(format!(
                "serving bundle format {} predates materialized entity aliases",
                manifest.format_version
            )));
        }
        Ok(())
    }

    pub async fn rollback_environment(
        &self,
        environment: CatalogEnvironment,
        release_id: &CatalogReleaseId,
        options: PromoteCatalogReleaseOptions,
    ) -> Result<EnvironmentPointer, CatalogReleaseError> {
        self.promote_environment(release_id, environment, options)
            .await
    }

    async fn validate_materialization_lineage(
        &self,
        release: &CatalogRelease,
    ) -> Result<Vec<QualityGateResult>, CatalogReleaseError> {
        let serving_asset =
            AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
        let serving_record = self
            .materializations
            .record_by_id_for_asset(
                &serving_asset,
                &release.derived_assets.serving_materialization_id,
            )
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(
                    release.derived_assets.serving_materialization_id.clone(),
                )
            })?;
        let pinned = &release.derived_assets;
        let kg_asset = AssetId::new(KG_SOCIETY_VIEW_ASSET_ID).expect("static asset id is valid");
        let kg_record = self
            .materializations
            .record_by_id_for_asset(&kg_asset, &pinned.kg_materialization_id)
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(pinned.kg_materialization_id.clone())
            })?;
        let project_asset =
            AssetId::new(CURRENT_PROJECT_FACTS_ASSET_ID).expect("static asset id is valid");
        let project_record = self
            .materializations
            .record_by_id_for_asset(&project_asset, &pinned.project_facts_materialization_id)
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(
                    pinned.project_facts_materialization_id.clone(),
                )
            })?;

        if serving_record.status != MaterializationStatus::Succeeded
            || kg_record.status != MaterializationStatus::Succeeded
            || project_record.status != MaterializationStatus::Succeeded
        {
            return Ok(vec![failed_gate(
                "pinned serving lineage contains a non-succeeded materialization".to_string(),
            )]);
        }
        if !serving_record
            .parent_materializations
            .contains(&pinned.kg_materialization_id)
        {
            return Ok(vec![failed_gate(format!(
                "serving materialization {} does not directly pin KG materialization {}",
                serving_record.materialization_id, pinned.kg_materialization_id
            ))]);
        }
        if !kg_record
            .parent_materializations
            .contains(&pinned.project_facts_materialization_id)
        {
            return Ok(vec![failed_gate(format!(
                "KG materialization {} does not directly pin project facts materialization {}",
                kg_record.materialization_id, pinned.project_facts_materialization_id
            ))]);
        }

        if let Err(error) =
            validate_search_serving_convergence(&self.materializations, &serving_record).await
        {
            return Ok(vec![failed_gate(error.to_string())]);
        }
        for parent_id in &serving_record.parent_materializations {
            if self
                .materializations
                .record_by_id(parent_id)
                .await?
                .is_none()
            {
                return Ok(vec![failed_gate(format!(
                    "serving materialization {} has missing direct parent {parent_id}",
                    serving_record.materialization_id
                ))]);
            }
        }

        let mut missing = Vec::new();
        for pinned_input in &release.pinned_inputs {
            let exists = self
                .materializations
                .record_by_id_for_asset(&pinned_input.asset_id, &pinned_input.materialization_id)
                .await?
                .is_some();
            if !exists {
                missing.push(format!(
                    "{}/{}",
                    pinned_input.asset_id, pinned_input.materialization_id
                ));
            }
        }
        if !missing.is_empty() {
            return Ok(vec![failed_gate(format!(
                "missing pinned input materializations: {}",
                missing.join(", ")
            ))]);
        }

        Ok(vec![passed_gate("pinned materialization lineage")])
    }

    async fn validate_serving_candidate(
        &self,
        release: &CatalogRelease,
    ) -> Result<Vec<QualityGateResult>, CatalogReleaseError> {
        let serving_asset =
            AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
        let serving_record = self
            .materializations
            .record_by_id_for_asset(
                &serving_asset,
                &release.derived_assets.serving_materialization_id,
            )
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(
                    release.derived_assets.serving_materialization_id.clone(),
                )
            })?;
        let Some(manifest_artifact) = serving_record
            .artifacts
            .iter()
            .find(|artifact| artifact.key.ends_with("manifest.json"))
        else {
            return Ok(vec![failed_gate(format!(
                "serving materialization {} has no manifest artifact",
                serving_record.materialization_id
            ))]);
        };
        let manifest_key = match LakeKey::new(manifest_artifact.key.clone()) {
            Ok(key) => key,
            Err(error) => return Ok(vec![failed_gate(error.to_string())]),
        };
        let manifest = match self
            .lake
            .get_json::<ServingBundleManifest>(&manifest_key)
            .await
        {
            Ok(manifest) => manifest,
            Err(error) => return Ok(vec![failed_gate(error.to_string())]),
        };
        if manifest.format_version < 8 {
            return Ok(vec![warning_gate(format!(
                "serving bundle format {} predates materialized entity aliases and cannot be promoted",
                manifest.format_version
            ))]);
        }

        match validate_search_serving_candidate(&self.lake, &serving_record).await {
            Ok(report) if report.passed => Ok(vec![passed_gate("complete serving candidate")]),
            Ok(report) => Ok(report
                .issues
                .into_iter()
                .map(|issue| {
                    failed_gate(match issue.reference {
                        Some(reference) => {
                            format!("{}: {} ({reference})", issue.code, issue.message)
                        }
                        None => format!("{}: {}", issue.code, issue.message),
                    })
                })
                .collect()),
            Err(error) => Ok(vec![failed_gate(error.to_string())]),
        }
    }

    async fn validate_memberships(
        &self,
        release: &CatalogRelease,
    ) -> Result<Vec<QualityGateResult>, CatalogReleaseError> {
        let mut gates = Vec::new();
        let mut property_ids = BTreeSet::new();
        let mut property_config_ids = BTreeSet::new();
        let mut rera_to_society = BTreeMap::<String, String>::new();
        let mut alias_to_society = BTreeMap::<String, String>::new();

        for membership in &release.memberships {
            if let Some(property_id) = normalized_optional(&membership.property_id) {
                if !property_ids.insert(property_id.clone()) {
                    gates.push(failed_gate(format!("duplicate property id {property_id}")));
                }
            }
            if let Some(config_id) = normalized_optional(&membership.property_config_id) {
                if !property_config_ids.insert(config_id.clone()) {
                    gates.push(failed_gate(format!(
                        "duplicate property configuration id {config_id}"
                    )));
                }
            }
            if let Some(rera_id) = normalized_optional(&membership.rera_id) {
                record_unique_mapping(
                    &mut gates,
                    &mut rera_to_society,
                    "RERA identity",
                    &rera_id,
                    &membership.society_id,
                );
            }
            for alias in &membership.aliases {
                if let Some(alias) = normalized_alias(alias) {
                    record_unique_mapping(
                        &mut gates,
                        &mut alias_to_society,
                        "alias",
                        &alias,
                        &membership.society_id,
                    );
                }
            }
            if let Some(coordinates) = &membership.coordinates {
                if !coordinates.latitude.is_finite()
                    || !coordinates.longitude.is_finite()
                    || !(-90.0..=90.0).contains(&coordinates.latitude)
                    || !(-180.0..=180.0).contains(&coordinates.longitude)
                {
                    gates.push(failed_gate(format!(
                        "invalid coordinates for society {}",
                        membership.society_id
                    )));
                }
            }
        }

        if let Some(base_release_id) = &release.base_release_id {
            let base = self.release(base_release_id).await?;
            let tombstones = release
                .tombstones
                .iter()
                .map(|tombstone| tombstone.entity_id.as_str())
                .collect::<BTreeSet<_>>();
            let release_properties = release
                .memberships
                .iter()
                .filter_map(|membership| membership.property_id.as_deref())
                .collect::<BTreeSet<_>>();
            for base_property in base
                .memberships
                .iter()
                .filter_map(|membership| membership.property_id.as_deref())
            {
                if !release_properties.contains(base_property)
                    && !tombstones.contains(base_property)
                {
                    gates.push(failed_gate(format!(
                        "base property {base_property} disappeared without tombstone"
                    )));
                }
            }

            let base_properties = base
                .memberships
                .iter()
                .filter_map(|membership| {
                    membership
                        .property_id
                        .as_deref()
                        .map(|property_id| (property_id, membership.society_id.as_str()))
                })
                .collect::<BTreeMap<_, _>>();
            let declared_additions = release
                .changes
                .added_societies
                .iter()
                .map(|society_id| normalize_society_id(society_id))
                .collect::<BTreeSet<_>>();
            let declared_removals = release
                .changes
                .removed_societies
                .iter()
                .map(|society_id| normalize_society_id(society_id))
                .collect::<BTreeSet<_>>();
            for membership in &release.memberships {
                let Some(property_id) = membership.property_id.as_deref() else {
                    continue;
                };
                if let Some(base_society_id) = base_properties.get(property_id) {
                    let base_society_id = normalize_society_id(base_society_id);
                    let next_society_id = normalize_society_id(&membership.society_id);
                    if base_society_id != next_society_id
                        && !(declared_removals.contains(&base_society_id)
                            && declared_additions.contains(&next_society_id))
                    {
                        gates.push(failed_gate(format!(
                            "property {property_id} changed society from {base_society_id} to {next_society_id} without a declared remove/add transition"
                        )));
                    }
                    continue;
                }
                if !declared_additions.contains(&normalize_society_id(&membership.society_id)) {
                    gates.push(failed_gate(format!(
                        "property {property_id} was added outside the base release without a declared society addition"
                    )));
                }
            }
        }

        if gates.is_empty() {
            gates.push(passed_gate("catalog membership identity"));
        }
        Ok(gates)
    }

    async fn validate_serving_rera_scope(
        &self,
        release: &CatalogRelease,
    ) -> Result<Vec<QualityGateResult>, CatalogReleaseError> {
        let serving_asset =
            AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
        let serving_record = self
            .materializations
            .record_by_id_for_asset(
                &serving_asset,
                &release.derived_assets.serving_materialization_id,
            )
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(
                    release.derived_assets.serving_materialization_id.clone(),
                )
            })?;
        let manifest_artifact = serving_record
            .artifacts
            .iter()
            .find(|artifact| artifact.key.ends_with("manifest.json"))
            .ok_or_else(|| {
                CatalogReleaseError::GateRejected(format!(
                    "serving materialization {} has no manifest artifact",
                    serving_record.materialization_id
                ))
            })?;
        let manifest_key = LakeKey::new(manifest_artifact.key.clone()).map_err(|err| {
            CatalogReleaseError::GateRejected(format!("invalid serving manifest key: {err}"))
        })?;
        let manifest: ServingBundleManifest = self.lake.get_json(&manifest_key).await?;
        let entities_key = LakeKey::new(manifest.entity_parquet_key.clone()).map_err(|err| {
            CatalogReleaseError::GateRejected(format!("invalid serving entities key: {err}"))
        })?;
        let entities = read_entities_parquet(&self.lake.get_bytes(&entities_key).await?)
            .map_err(|err| CatalogReleaseError::GateRejected(err.to_string()))?;
        let Some(rera_key) = manifest.rera_evidence_parquet_key.as_ref() else {
            return Ok(vec![passed_gate("serving RERA evidence scope")]);
        };
        let rera_key = LakeKey::new(rera_key.clone()).map_err(|err| {
            CatalogReleaseError::GateRejected(format!("invalid serving RERA evidence key: {err}"))
        })?;
        let evidence = read_rera_evidence_parquet(&self.lake.get_bytes(&rera_key).await?)
            .map_err(|err| CatalogReleaseError::GateRejected(err.to_string()))?;

        let society_ids = entities
            .iter()
            .filter(|entity| entity.entity_type == "society")
            .map(|entity| entity.entity_id.clone())
            .collect::<BTreeSet<_>>();
        let aliases = unique_society_aliases(&entities)
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let membership_societies = release
            .memberships
            .iter()
            .filter_map(|membership| {
                let normalized = normalize_society_id(&membership.society_id);
                if society_ids.contains(&normalized) {
                    Some(normalized)
                } else {
                    aliases.get(&normalized).cloned()
                }
            })
            .collect::<BTreeSet<_>>();
        let mut mapped_registrations = BTreeMap::<String, String>::new();
        let mut gates = Vec::new();
        for record in &evidence {
            let canonical_id = if society_ids.contains(&record.society_id) {
                Some(record.society_id.clone())
            } else {
                aliases.get(&record.society_id).cloned()
            };
            let Some(canonical_id) = canonical_id else {
                gates.push(failed_gate(format!(
                    "RERA evidence society {} is absent from serving entities",
                    record.society_id
                )));
                continue;
            };
            if !membership_societies.contains(&canonical_id) {
                gates.push(failed_gate(format!(
                    "RERA evidence society {} has no catalog membership",
                    record.society_id
                )));
            }
            for registration_id in &record.registration_ids {
                record_unique_mapping(
                    &mut gates,
                    &mut mapped_registrations,
                    "serving RERA registration",
                    registration_id,
                    &canonical_id,
                );
            }
        }
        if gates.is_empty() {
            gates.push(passed_gate("serving RERA evidence scope"));
        }
        if !manifest.excluded_rera_evidence_society_ids.is_empty() {
            gates.push(warning_gate(format!(
                "{} collected RERA societ{} outside this catalog and omitted from serving: {}",
                manifest.excluded_rera_evidence_society_ids.len(),
                if manifest.excluded_rera_evidence_society_ids.len() == 1 {
                    "y was"
                } else {
                    "ies were"
                },
                manifest.excluded_rera_evidence_society_ids.join(", ")
            )));
        }
        Ok(gates)
    }

    async fn validate_serving_projection_memberships(
        &self,
        release: &CatalogRelease,
    ) -> Result<Vec<QualityGateResult>, CatalogReleaseError> {
        let serving_asset =
            AssetId::new(SEARCH_SERVING_BUNDLE_ASSET_ID).expect("static asset id is valid");
        let serving_record = self
            .materializations
            .record_by_id_for_asset(
                &serving_asset,
                &release.derived_assets.serving_materialization_id,
            )
            .await?
            .ok_or_else(|| {
                CatalogReleaseError::MissingMaterialization(
                    release.derived_assets.serving_materialization_id.clone(),
                )
            })?;
        let manifest_artifact = serving_record
            .artifacts
            .iter()
            .find(|artifact| artifact.key.ends_with("manifest.json"))
            .ok_or_else(|| {
                CatalogReleaseError::GateRejected(format!(
                    "serving materialization {} has no manifest artifact",
                    serving_record.materialization_id
                ))
            })?;
        let manifest_key = LakeKey::new(manifest_artifact.key.clone()).map_err(|err| {
            CatalogReleaseError::GateRejected(format!("invalid serving manifest key: {err}"))
        })?;
        let manifest: ServingBundleManifest = self.lake.get_json(&manifest_key).await?;
        if manifest.format_version < 8 {
            return Ok(vec![warning_gate(format!(
                "serving bundle format {} predates materialized entity aliases and cannot be promoted",
                manifest.format_version
            ))]);
        }

        let entities = read_entities_parquet(
            &self
                .lake
                .get_bytes(
                    &LakeKey::new(manifest.entity_parquet_key.clone()).map_err(|err| {
                        CatalogReleaseError::GateRejected(format!(
                            "invalid serving entities key: {err}"
                        ))
                    })?,
                )
                .await?,
        )
        .map_err(|err| CatalogReleaseError::GateRejected(err.to_string()))?;
        let facts = read_facts_parquet(
            &self
                .lake
                .get_bytes(
                    &LakeKey::new(manifest.fact_parquet_key.clone()).map_err(|err| {
                        CatalogReleaseError::GateRejected(format!(
                            "invalid serving facts key: {err}"
                        ))
                    })?,
                )
                .await?,
        )
        .map_err(|err| CatalogReleaseError::GateRejected(err.to_string()))?;
        let metadata = read_search_metadata_parquet(
            &self
                .lake
                .get_bytes(
                    &LakeKey::new(manifest.search_metadata_parquet_key.clone()).map_err(|err| {
                        CatalogReleaseError::GateRejected(format!(
                            "invalid serving search metadata key: {err}"
                        ))
                    })?,
                )
                .await?,
        )
        .map_err(|err| CatalogReleaseError::GateRejected(err.to_string()))?;
        let edges = match manifest.edge_parquet_key.as_ref() {
            Some(key) => read_edges_parquet(
                &self
                    .lake
                    .get_bytes(&LakeKey::new(key.clone()).map_err(|err| {
                        CatalogReleaseError::GateRejected(format!(
                            "invalid serving edges key: {err}"
                        ))
                    })?)
                    .await?,
            )
            .map_err(|err| CatalogReleaseError::GateRejected(err.to_string()))?,
            None => Vec::new(),
        };
        let mut fact_index = ServingFactIndex::from_records(facts, metadata);
        fact_index.add_society_aliases(&entities);
        let properties = crate::data_loader::properties_from_serving_records_with_edges(
            &entities,
            &edges,
            &fact_index,
            &manifest.bundle_version,
        );

        let projected = properties
            .iter()
            .map(|property| {
                (
                    property.id.trim().to_ascii_lowercase(),
                    normalize_runtime_society_id(&property.society_id),
                )
            })
            .collect::<BTreeSet<_>>();
        let mut declared = BTreeSet::new();
        let mut gates = Vec::new();
        for membership in &release.memberships {
            let Some(property_id) = normalized_optional(&membership.property_id) else {
                gates.push(failed_gate(format!(
                    "catalog membership for {} has no property id",
                    membership.society_id
                )));
                continue;
            };
            let Some(config_id) = normalized_optional(&membership.property_config_id) else {
                gates.push(failed_gate(format!(
                    "catalog property {property_id} has no property configuration id"
                )));
                continue;
            };
            if config_id != property_id {
                gates.push(failed_gate(format!(
                    "catalog property {property_id} has mismatched configuration id {config_id}"
                )));
            }
            declared.insert((
                property_id.to_ascii_lowercase(),
                normalize_runtime_society_id(&membership.society_id),
            ));
        }

        let missing = declared
            .difference(&projected)
            .take(5)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            gates.push(failed_gate(format!(
                "catalog memberships absent from the serving property projection: {}",
                format_property_pairs(&missing)
            )));
        }
        let undeclared = projected
            .difference(&declared)
            .take(5)
            .cloned()
            .collect::<Vec<_>>();
        if !undeclared.is_empty() {
            gates.push(failed_gate(format!(
                "serving property projection contains undeclared catalog memberships: {}",
                format_property_pairs(&undeclared)
            )));
        }
        if gates.is_empty() {
            gates.push(passed_gate("serving property projection parity"));
        }
        Ok(gates)
    }
}

fn normalize_society_id(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    if let Some(slug) = normalized.strip_prefix("soc-") {
        format!("society:{slug}")
    } else if normalized.starts_with("society:") {
        normalized
    } else {
        format!("society:{normalized}")
    }
}

#[derive(Debug, Clone, Default)]
pub struct PromoteCatalogReleaseOptions {
    pub expected_current_release: Option<CatalogReleaseId>,
    pub approve_production: bool,
    pub force_legacy_pointer: bool,
}

#[derive(Debug)]
pub enum CatalogReleaseError {
    MissingMaterialization(MaterializationId),
    GateRejected(String),
    PromotionRejected {
        environment: CatalogEnvironment,
        desired: CatalogReleaseId,
        current: Option<CatalogReleaseId>,
    },
    Lake(LakeError),
    ServingLineage(ServingReleasePromotionError),
}

impl fmt::Display for CatalogReleaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingMaterialization(id) => {
                write!(formatter, "missing materialization {id}")
            }
            Self::GateRejected(message) => formatter.write_str(message),
            Self::PromotionRejected {
                environment,
                desired,
                current,
            } => write!(
                formatter,
                "promotion of release {desired} to {environment} was rejected because current is {}",
                current
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "unset".to_string())
            ),
            Self::Lake(error) => error.fmt(formatter),
            Self::ServingLineage(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CatalogReleaseError {}

impl From<LakeError> for CatalogReleaseError {
    fn from(error: LakeError) -> Self {
        Self::Lake(error)
    }
}

impl From<ServingReleasePromotionError> for CatalogReleaseError {
    fn from(error: ServingReleasePromotionError) -> Self {
        Self::ServingLineage(error)
    }
}

pub fn release_key(release_id: &CatalogReleaseId) -> LakeKey {
    LakeKey::join(&[
        "manifests",
        "catalog",
        "releases",
        &format!("{release_id}.json"),
    ])
    .expect("catalog release key is valid")
}

pub fn environment_pointer_key(environment: CatalogEnvironment) -> LakeKey {
    LakeKey::join(&[
        "manifests",
        "catalog",
        "environments",
        &format!("{environment}.json"),
    ])
    .expect("catalog environment pointer key is valid")
}

fn passed_gate(gate: &str) -> QualityGateResult {
    QualityGateResult {
        gate: gate.to_string(),
        status: QualityGateStatus::Passed,
        message: None,
    }
}

fn failed_gate(message: String) -> QualityGateResult {
    QualityGateResult {
        gate: "catalog structural validation".to_string(),
        status: QualityGateStatus::Failed,
        message: Some(message),
    }
}

fn warning_gate(message: String) -> QualityGateResult {
    QualityGateResult {
        gate: "catalog serving coverage".to_string(),
        status: QualityGateStatus::Warning,
        message: Some(message),
    }
}

fn normalized_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_runtime_society_id(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    if let Some(slug) = normalized.strip_prefix("society:") {
        format!("soc-{slug}")
    } else if normalized.starts_with("soc-") {
        normalized
    } else {
        format!("soc-{normalized}")
    }
}

fn format_property_pairs(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(property_id, society_id)| format!("{property_id} -> {society_id}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalized_alias(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn record_unique_mapping(
    gates: &mut Vec<QualityGateResult>,
    mappings: &mut BTreeMap<String, String>,
    label: &str,
    key: &str,
    society_id: &str,
) {
    if let Some(existing) = mappings.insert(key.to_string(), society_id.to_string()) {
        if existing != society_id {
            gates.push(failed_gate(format!(
                "{label} {key} maps to both {existing} and {society_id}"
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::assets::{ArtifactRef, AssetPartition, AssetStage, MaterializationRecord};
    use crate::lake::LakeStore;
    use crate::serving::parquet::write_entities_parquet;
    use crate::serving::{
        write_rera_evidence_parquet, ServingBundleManifest, ServingEntityRecord,
        ServingReraEvidenceRecord,
    };
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn validates_pinned_serving_lineage() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let derived = write_test_lineage(&lake, &[], &[], Vec::new()).await;

        let mut release =
            CatalogRelease::candidate(None, Some("release zero".to_string()), derived);
        release.memberships.push(CatalogMembership {
            society_id: "society:one".to_string(),
            property_id: Some("property:one-3bhk".to_string()),
            property_config_id: Some("one-3bhk".to_string()),
            rera_id: Some("PRM/KA/RERA/1251/310/PR/1".to_string()),
            aliases: vec!["Society One".to_string()],
            coordinates: Some(CatalogCoordinates {
                latitude: 12.9,
                longitude: 77.6,
            }),
            source_materialization_id: None,
            membership_kind: CatalogMembershipKind::Reused,
        });
        store.write_release(&release).await.unwrap();
        let validated = store.validate_release(&release.release_id).await.unwrap();
        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Validated
        );
        assert_eq!(
            validated.quality_report.as_ref().unwrap().status,
            QualityGateStatus::Passed
        );
    }

    #[tokio::test]
    async fn rejects_release_when_current_project_facts_missed_a_current_dag_partition() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let materializations = AssetMaterializationStore::new(lake.clone());
        let derived = write_test_lineage(&lake, &[], &[], Vec::new()).await;

        let image_media = MaterializationRecord::succeeded(
            AssetId::new("image_media_facts").unwrap(),
            AssetStage::Silver,
            AssetPartition::new([("source", "external_image")]),
            "new-media",
            Vec::new(),
        );
        materializations
            .write_materialization(&image_media)
            .await
            .unwrap();
        materializations
            .force_promote_current(&image_media)
            .await
            .unwrap();

        let mut release = CatalogRelease::candidate(None, None, derived);
        release.memberships.push(membership("property:one"));
        store.write_release(&release).await.unwrap();

        let validated = store.validate_release(&release.release_id).await.unwrap();

        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Rejected
        );
        assert!(quality_messages(&validated).any(|message| {
            message.contains("did not converge current DAG inputs")
                && message.contains("image_media_facts")
        }));
    }

    #[tokio::test]
    async fn rejects_base_property_disappearance_without_tombstone() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let derived = write_test_lineage(&lake, &[], &[], Vec::new()).await;
        let mut base = CatalogRelease::candidate(None, None, derived.clone());
        base.memberships.push(membership("property:keep"));
        store.write_release(&base).await.unwrap();

        let child = CatalogRelease::candidate(Some(base.release_id.clone()), None, derived);
        store.write_release(&child).await.unwrap();
        let validated = store.validate_release(&child.release_id).await.unwrap();
        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Rejected
        );
        assert!(validated
            .quality_report
            .as_ref()
            .unwrap()
            .gates
            .iter()
            .any(|gate| gate
                .message
                .as_deref()
                .is_some_and(|message| message.contains("disappeared without tombstone"))));
    }

    #[tokio::test]
    async fn rejects_undeclared_membership_addition_to_base_release() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let derived = write_test_lineage(&lake, &[], &[], Vec::new()).await;

        let mut base = CatalogRelease::candidate(None, None, derived.clone());
        base.memberships.push(membership("property:existing"));
        store.write_release(&base).await.unwrap();

        let mut child = CatalogRelease::candidate(Some(base.release_id.clone()), None, derived);
        child.memberships.extend([
            membership("property:existing"),
            membership("property:unexpected"),
        ]);
        store.write_release(&child).await.unwrap();

        let validated = store.validate_release(&child.release_id).await.unwrap();
        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Rejected
        );
        assert!(quality_messages(&validated).any(|message| {
            message.contains("property:unexpected") && message.contains("without a declared")
        }));
    }

    #[tokio::test]
    async fn rejects_undeclared_property_society_remap() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let derived = write_test_lineage(&lake, &[], &[], Vec::new()).await;

        let mut base = CatalogRelease::candidate(None, None, derived.clone());
        base.memberships.push(membership("property:existing"));
        store.write_release(&base).await.unwrap();

        let mut child = CatalogRelease::candidate(Some(base.release_id.clone()), None, derived);
        let mut remapped = membership("property:existing");
        remapped.society_id = "society:two".to_string();
        child.memberships.push(remapped);
        store.write_release(&child).await.unwrap();

        let validated = store.validate_release(&child.release_id).await.unwrap();
        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Rejected
        );
        assert!(quality_messages(&validated).any(|message| {
            message.contains("property:existing")
                && message.contains("changed society")
                && message.contains("without a declared remove/add transition")
        }));
    }

    #[tokio::test]
    async fn rejects_rera_evidence_outside_serving_entities() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let derived = write_test_lineage(
            &lake,
            &[society_entity("society:one")],
            &[rera_evidence("society:orphan", "registration:orphan")],
            Vec::new(),
        )
        .await;
        let mut release = CatalogRelease::candidate(None, None, derived);
        release.memberships.push(membership("property:one"));
        store.write_release(&release).await.unwrap();

        let validated = store.validate_release(&release.release_id).await.unwrap();
        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Rejected
        );
        assert!(quality_messages(&validated)
            .any(|message| message.contains("society:orphan is absent from serving entities")));
    }

    #[tokio::test]
    async fn records_excluded_rera_societies_as_release_warning() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let derived = write_test_lineage(
            &lake,
            &[society_entity("society:one")],
            &[rera_evidence("society:one", "registration:one")],
            vec!["society:outside-catalog".to_string()],
        )
        .await;
        let mut release = CatalogRelease::candidate(None, None, derived);
        release.memberships.push(membership("property:one"));
        store.write_release(&release).await.unwrap();

        let validated = store.validate_release(&release.release_id).await.unwrap();
        assert_eq!(
            validated.validation_status,
            CatalogValidationStatus::Validated
        );
        assert!(validated
            .quality_report
            .as_ref()
            .unwrap()
            .gates
            .iter()
            .any(|gate| {
                gate.status == QualityGateStatus::Warning
                    && gate
                        .message
                        .as_deref()
                        .is_some_and(|message| message.contains("society:outside-catalog"))
            }));
    }

    #[tokio::test]
    async fn production_promotion_requires_staging_and_approval() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let materializations = AssetMaterializationStore::new(lake.clone());
        let project = record(CURRENT_PROJECT_FACTS_ASSET_ID, Vec::new());
        let kg = record(
            KG_SOCIETY_VIEW_ASSET_ID,
            vec![project.materialization_id.clone()],
        );
        let mut serving = record(
            SEARCH_SERVING_BUNDLE_ASSET_ID,
            vec![kg.materialization_id.clone()],
        );
        let manifest_key = LakeKey::new("serving/test-promotion/manifest.json").unwrap();
        let manifest = ServingBundleManifest {
            bundle_version: "test-promotion".to_string(),
            format_version: 8,
            created_at: Utc::now(),
            entity_count: 0,
            entity_alias_count: 0,
            fact_count: 0,
            search_metadata_count: 0,
            rera_evidence_count: 0,
            excluded_rera_evidence_society_ids: Vec::new(),
            edge_count: 0,
            eligibility_policy_version: 1,
            quarantined_society_count: 0,
            quarantine_reason_counts: BTreeMap::new(),
            entity_parquet_key: String::new(),
            entity_alias_parquet_key: None,
            fact_parquet_key: String::new(),
            search_metadata_parquet_key: String::new(),
            rera_evidence_parquet_key: None,
            edge_parquet_key: None,
            quarantine_report_key: None,
            schema_key: String::new(),
            trust_policy_key: String::new(),
            tantivy_index_prefix: String::new(),
            artifacts: Vec::new(),
        };
        serving.artifacts = vec![ArtifactRef::json(
            lake.put_json(&manifest_key, &manifest).await.unwrap(),
        )];
        for record in [&project, &kg, &serving] {
            materializations
                .write_materialization(record)
                .await
                .unwrap();
        }
        let release = CatalogRelease {
            validation_status: CatalogValidationStatus::Validated,
            quality_report: Some(QualityReport::passed(vec![passed_gate("test")])),
            ..CatalogRelease::candidate(
                None,
                None,
                DerivedCatalogAssets {
                    serving_materialization_id: serving.materialization_id.clone(),
                    kg_materialization_id: kg.materialization_id.clone(),
                    project_facts_materialization_id: project.materialization_id.clone(),
                    materializations: Vec::new(),
                },
            )
        };
        store.write_release(&release).await.unwrap();

        assert!(store
            .promote_environment(
                &release.release_id,
                CatalogEnvironment::Production,
                PromoteCatalogReleaseOptions {
                    approve_production: true,
                    expected_current_release: None,
                    force_legacy_pointer: false,
                },
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("staging"));

        store
            .promote_environment(
                &release.release_id,
                CatalogEnvironment::Staging,
                PromoteCatalogReleaseOptions::default(),
            )
            .await
            .unwrap();
        assert!(store
            .promote_environment(
                &release.release_id,
                CatalogEnvironment::Production,
                PromoteCatalogReleaseOptions {
                    approve_production: false,
                    expected_current_release: None,
                    force_legacy_pointer: false,
                },
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("approve"));

        let production = store
            .promote_environment(
                &release.release_id,
                CatalogEnvironment::Production,
                PromoteCatalogReleaseOptions {
                    approve_production: true,
                    expected_current_release: None,
                    force_legacy_pointer: false,
                },
            )
            .await
            .unwrap();
        assert_eq!(production.release_id, release.release_id);
    }

    #[tokio::test]
    async fn promotion_rejects_bundle_without_eligibility_quarantine_format() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogReleaseStore::new(lake.clone());
        let materializations = AssetMaterializationStore::new(lake.clone());
        let mut serving = record(SEARCH_SERVING_BUNDLE_ASSET_ID, Vec::new());
        let manifest_key = LakeKey::new("serving/legacy/manifest.json").unwrap();
        let manifest = ServingBundleManifest {
            bundle_version: "legacy".to_string(),
            format_version: 6,
            created_at: Utc::now(),
            entity_count: 0,
            entity_alias_count: 0,
            fact_count: 0,
            search_metadata_count: 0,
            rera_evidence_count: 0,
            excluded_rera_evidence_society_ids: Vec::new(),
            edge_count: 0,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: BTreeMap::new(),
            entity_parquet_key: String::new(),
            entity_alias_parquet_key: None,
            fact_parquet_key: String::new(),
            search_metadata_parquet_key: String::new(),
            rera_evidence_parquet_key: None,
            edge_parquet_key: None,
            quarantine_report_key: None,
            schema_key: String::new(),
            trust_policy_key: String::new(),
            tantivy_index_prefix: String::new(),
            artifacts: Vec::new(),
        };
        serving.artifacts = vec![ArtifactRef::json(
            lake.put_json(&manifest_key, &manifest).await.unwrap(),
        )];
        materializations
            .write_materialization(&serving)
            .await
            .unwrap();
        let release = CatalogRelease {
            validation_status: CatalogValidationStatus::Validated,
            quality_report: Some(QualityReport::passed(vec![passed_gate("legacy test")])),
            ..CatalogRelease::candidate(
                None,
                None,
                DerivedCatalogAssets {
                    serving_materialization_id: serving.materialization_id,
                    kg_materialization_id: MaterializationId::new(),
                    project_facts_materialization_id: MaterializationId::new(),
                    materializations: Vec::new(),
                },
            )
        };
        store.write_release(&release).await.unwrap();

        let error = store
            .promote_environment(
                &release.release_id,
                CatalogEnvironment::Dev,
                PromoteCatalogReleaseOptions::default(),
            )
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("predates materialized entity aliases"));
        assert!(store
            .current_pointer(CatalogEnvironment::Dev)
            .await
            .unwrap()
            .is_none());
    }

    fn membership(property_id: &str) -> CatalogMembership {
        CatalogMembership {
            society_id: "society:one".to_string(),
            property_id: Some(property_id.to_string()),
            property_config_id: None,
            rera_id: None,
            aliases: Vec::new(),
            coordinates: None,
            source_materialization_id: None,
            membership_kind: CatalogMembershipKind::Reused,
        }
    }

    fn society_entity(entity_id: &str) -> ServingEntityRecord {
        ServingEntityRecord {
            entity_id: entity_id.to_string(),
            entity_type: "society".to_string(),
            name: entity_id.to_string(),
            root_source: None,
            searchable_text: String::new(),
        }
    }

    fn rera_evidence(society_id: &str, registration_id: &str) -> ServingReraEvidenceRecord {
        ServingReraEvidenceRecord {
            society_id: society_id.to_string(),
            registration_ids: vec![registration_id.to_string()],
            entities: Vec::new(),
            claims: Vec::new(),
            events: Vec::new(),
            series: Vec::new(),
            discrepancies: Vec::new(),
            regulatory_coverage: Vec::new(),
            source_index: Vec::new(),
        }
    }

    fn quality_messages(release: &CatalogRelease) -> impl Iterator<Item = &str> {
        release
            .quality_report
            .as_ref()
            .into_iter()
            .flat_map(|report| report.gates.iter())
            .filter_map(|gate| gate.message.as_deref())
    }

    fn record(asset_id: &str, parents: Vec<MaterializationId>) -> MaterializationRecord {
        MaterializationRecord::succeeded(
            AssetId::new(asset_id).unwrap(),
            match asset_id {
                SEARCH_SERVING_BUNDLE_ASSET_ID => AssetStage::Serving,
                _ => AssetStage::Gold,
            },
            AssetPartition::global(),
            "test",
            Vec::new(),
        )
        .with_parent_materializations(parents)
    }

    async fn write_test_lineage(
        lake: &LakeStore,
        entities: &[ServingEntityRecord],
        evidence: &[ServingReraEvidenceRecord],
        excluded_rera_evidence_society_ids: Vec<String>,
    ) -> DerivedCatalogAssets {
        let materializations = AssetMaterializationStore::new(lake.clone());
        let project = record(CURRENT_PROJECT_FACTS_ASSET_ID, Vec::new());
        let kg = record(
            KG_SOCIETY_VIEW_ASSET_ID,
            vec![project.materialization_id.clone()],
        );
        let mut serving = record(
            SEARCH_SERVING_BUNDLE_ASSET_ID,
            vec![kg.materialization_id.clone()],
        );
        let prefix = format!("serving/test/{}", serving.materialization_id);
        let entities_key = LakeKey::new(format!("{prefix}/entities.parquet")).unwrap();
        let evidence_key = LakeKey::new(format!("{prefix}/rera_evidence.parquet")).unwrap();
        lake.put_bytes(&entities_key, write_entities_parquet(entities).unwrap())
            .await
            .unwrap();
        lake.put_bytes(
            &evidence_key,
            write_rera_evidence_parquet(evidence).unwrap(),
        )
        .await
        .unwrap();
        let manifest = ServingBundleManifest {
            bundle_version: "test".to_string(),
            format_version: 1,
            created_at: Utc::now(),
            entity_count: entities.len() as u64,
            entity_alias_count: 0,
            fact_count: 0,
            search_metadata_count: 0,
            rera_evidence_count: evidence.len() as u64,
            excluded_rera_evidence_society_ids,
            edge_count: 0,
            eligibility_policy_version: 0,
            quarantined_society_count: 0,
            quarantine_reason_counts: BTreeMap::new(),
            entity_parquet_key: entities_key.to_string(),
            entity_alias_parquet_key: None,
            fact_parquet_key: format!("{prefix}/facts.parquet"),
            search_metadata_parquet_key: format!("{prefix}/search_metadata.parquet"),
            rera_evidence_parquet_key: Some(evidence_key.to_string()),
            edge_parquet_key: None,
            quarantine_report_key: None,
            schema_key: format!("{prefix}/schema.json"),
            trust_policy_key: format!("{prefix}/trust.json"),
            tantivy_index_prefix: format!("{prefix}/tantivy"),
            artifacts: Vec::new(),
        };
        let manifest_key = LakeKey::new(format!("{prefix}/manifest.json")).unwrap();
        let metadata = lake.put_json(&manifest_key, &manifest).await.unwrap();
        serving.artifacts = vec![ArtifactRef::json(metadata)];
        for record in [&project, &kg, &serving] {
            materializations
                .write_materialization(record)
                .await
                .unwrap();
        }
        DerivedCatalogAssets {
            serving_materialization_id: serving.materialization_id,
            kg_materialization_id: kg.materialization_id,
            project_facts_materialization_id: project.materialization_id,
            materializations: Vec::new(),
        }
    }
}
