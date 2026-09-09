//! Small, atomic catalog lifecycle for society-partitioned gold and one serving bundle.
//!
//! The catalog control plane is deliberately one pointer. Each pointer generation
//! names an immutable roster and an immutable format-12 serving bundle. Society
//! gold snapshots are also immutable; replacing a society only changes the roster
//! entry selected by the next generation.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::assets::{MaterializationId, SourceEntitySeed};
use crate::lake::{ArtifactMetadata, LakeError, LakeKey, LakePrefix, LakeStore};
#[cfg(test)]
use crate::serving::ServingBundleManifest;
use crate::serving::{
    current_serving_annotations, current_serving_facts, project_rera_evidence, read_edges_parquet,
    read_entities_parquet, read_facts_parquet, read_rera_evidence_parquet,
    read_search_metadata_parquet, serving_edge_records, serving_entity_records,
    serving_fact_records, serving_search_metadata_records, validate_search_serving_candidate,
    write_edges_parquet, write_entities_parquet, write_facts_parquet, write_rera_evidence_parquet,
    write_search_metadata_parquet, ServingBundleBuilder, ServingBundleError,
    ServingBundleLoadError, ServingBundleLoader, ServingEdgeRecord, ServingEntityRecord,
    ServingFactRecord, ServingReraEvidenceRecord, ServingSearchMetadataRecord,
    SERVING_BUNDLE_FORMAT_VERSION,
};
use crate::{
    assets::{
        load_society_gold_records, openestates_registry, read_rera_claims,
        read_rera_receipt_records, read_rera_source_records, AssetDagExecutionOptions,
        AssetDagExecutor, AssetId, AssetMaterializationStore, AssetPartition, AssetRunStepStatus,
        AssetSourceInputs, CommandSourceInputProvider, SocietyGoldManifest, SocietyGoldRecords,
        SourceEntityResolutionScope, SourceInputProvider, SourceInputRequest, RERA_CLAIMS_ASSET_ID,
        RERA_RECEIPTS_ASSET_ID, RERA_SOURCE_RECORDS_ASSET_ID, SOCIETY_GOLD_SNAPSHOT_ASSET_ID,
    },
    knowledge::KnowledgeGraph,
};

pub const CATALOG_FORMAT_VERSION: u32 = 1;
const GOLD_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRosterEntry {
    pub seed: SourceEntitySeed,
    pub snapshot_id: MaterializationId,
    pub snapshot_manifest_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogTopologyEntry {
    pub snapshot_id: MaterializationId,
    pub snapshot_manifest_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRoster {
    pub format_version: u32,
    pub roster_id: MaterializationId,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<CatalogTopologyEntry>,
    pub societies: Vec<CatalogRosterEntry>,
}

impl CatalogRoster {
    pub fn empty() -> Self {
        Self {
            format_version: CATALOG_FORMAT_VERSION,
            roster_id: MaterializationId::new(),
            created_at: Utc::now(),
            topology: None,
            societies: Vec::new(),
        }
    }

    fn normalized(mut self) -> Result<Self, CatalogError> {
        self.format_version = CATALOG_FORMAT_VERSION;
        self.roster_id = MaterializationId::new();
        self.created_at = Utc::now();
        self.societies.sort_by(|left, right| {
            canonical_seed_id(&left.seed).cmp(canonical_seed_id(&right.seed))
        });
        let mut identities = HashSet::new();
        for entry in &self.societies {
            validate_seed(&entry.seed)?;
            for identity in seed_identities(&entry.seed) {
                if !identities.insert(identity.clone()) {
                    return Err(CatalogError::Invalid(format!(
                        "duplicate canonical catalog identity {identity}"
                    )));
                }
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogGeneration {
    pub roster_id: MaterializationId,
    pub roster_key: String,
    pub bundle_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPointer {
    pub format_version: u32,
    pub revision: u64,
    pub current: CatalogGeneration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<CatalogGeneration>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogOperationReport {
    pub operation: String,
    pub revision: u64,
    pub society_count: usize,
    pub bundle_version: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocietyGoldSnapshotManifest {
    pub format_version: u32,
    pub snapshot_id: MaterializationId,
    pub society_id: String,
    pub seed: SourceEntitySeed,
    pub created_at: DateTime<Utc>,
    pub entity_count: u64,
    pub fact_count: u64,
    pub search_metadata_count: u64,
    pub edge_count: u64,
    pub rera_evidence_count: u64,
    pub artifacts: Vec<CatalogArtifact>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl SocietyGoldSnapshotManifest {
    pub fn roster_entry(&self) -> CatalogRosterEntry {
        CatalogRosterEntry {
            seed: self.seed.clone(),
            snapshot_id: self.snapshot_id.clone(),
            snapshot_manifest_key: snapshot_manifest_key(&self.seed, &self.snapshot_id).to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogTopologySnapshotManifest {
    pub format_version: u32,
    pub snapshot_id: MaterializationId,
    pub created_at: DateTime<Utc>,
    pub entity_count: u64,
    pub fact_count: u64,
    pub search_metadata_count: u64,
    pub edge_count: u64,
    pub rera_evidence_count: u64,
    pub artifacts: Vec<CatalogArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogArtifact {
    pub kind: CatalogArtifactKind,
    pub key: String,
    pub content_hash: String,
    pub size_bytes: usize,
    pub row_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogArtifactKind {
    Entities,
    Facts,
    SearchMetadata,
    Edges,
    ReraEvidence,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CatalogRecords {
    pub entities: Vec<ServingEntityRecord>,
    pub facts: Vec<ServingFactRecord>,
    pub search_metadata: Vec<ServingSearchMetadataRecord>,
    pub edges: Vec<ServingEdgeRecord>,
    pub rera_evidence: Vec<ServingReraEvidenceRecord>,
}

impl CatalogRecords {
    pub fn from_society_gold(
        gold: &SocietyGoldRecords,
        rera_evidence: Vec<ServingReraEvidenceRecord>,
    ) -> Result<Self, CatalogError> {
        let current_facts = current_serving_facts(&gold.facts);
        let current_annotations = current_serving_annotations(&gold.fact_annotations);
        Ok(Self {
            entities: serving_entity_records(gold, &current_facts),
            facts: serving_fact_records(&current_facts).map_err(|error| {
                CatalogError::Invalid(format!("society facts are invalid: {error}"))
            })?,
            search_metadata: serving_search_metadata_records(&current_facts, &current_annotations)
                .map_err(|error| {
                    CatalogError::Invalid(format!("society search metadata is invalid: {error}"))
                })?,
            edges: serving_edge_records(&gold.edges),
            rera_evidence,
        })
    }

    /// Extract one society and its property rows. Support entities referenced by
    /// its direct relations are retained so the snapshot remains self-contained.
    fn for_society(&self, seed: &SourceEntitySeed) -> Result<Self, CatalogError> {
        validate_seed(seed)?;
        let identities = seed_identities(seed);
        let normalized_name = normalize_identity(&seed.name);
        let society_ids = self
            .entities
            .iter()
            .filter(|entity| {
                entity.entity_type == "society"
                    && (identities.contains(&normalize_identity(&entity.entity_id))
                        || normalize_identity(&entity.name) == normalized_name)
            })
            .map(|entity| entity.entity_id.clone())
            .collect::<BTreeSet<_>>();
        if society_ids.len() != 1 {
            return Err(CatalogError::Invalid(format!(
                "society seed {} resolved to {} serving society identities",
                seed.name,
                society_ids.len()
            )));
        }
        let society_id = society_ids.iter().next().expect("one society");
        let mut primary_ids = BTreeSet::from([society_id.clone()]);
        primary_ids.extend(
            self.edges
                .iter()
                .filter(|edge| edge.edge_type == "in_society" && edge.to_entity_id == *society_id)
                .map(|edge| edge.from_entity_id.clone()),
        );

        let incident_edges = self
            .edges
            .iter()
            .filter(|edge| {
                primary_ids.contains(&edge.from_entity_id)
                    || primary_ids.contains(&edge.to_entity_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut retained_ids = primary_ids.clone();
        for edge in &incident_edges {
            retained_ids.insert(edge.from_entity_id.clone());
            retained_ids.insert(edge.to_entity_id.clone());
        }
        let records = Self {
            entities: self
                .entities
                .iter()
                .filter(|entity| retained_ids.contains(&entity.entity_id))
                .cloned()
                .collect(),
            facts: self
                .facts
                .iter()
                .filter(|fact| retained_ids.contains(&fact.entity_id))
                .cloned()
                .collect(),
            search_metadata: self
                .search_metadata
                .iter()
                .filter(|row| retained_ids.contains(&row.entity_id))
                .cloned()
                .collect(),
            edges: incident_edges,
            rera_evidence: self
                .rera_evidence
                .iter()
                .filter(|record| {
                    identities.contains(&normalize_identity(&record.society_id))
                        || record.society_id == *society_id
                })
                .cloned()
                .collect(),
        };
        records.validate_society_scope(society_id)?;
        Ok(records)
    }

    fn validate_society_scope(&self, society_id: &str) -> Result<(), CatalogError> {
        let property_ids = self
            .entities
            .iter()
            .filter(|entity| entity.entity_type == "property")
            .map(|entity| entity.entity_id.as_str())
            .collect::<BTreeSet<_>>();
        for property_id in property_ids {
            let society_links = self
                .edges
                .iter()
                .filter(|edge| edge.edge_type == "in_society" && edge.from_entity_id == property_id)
                .map(|edge| edge.to_entity_id.as_str())
                .collect::<BTreeSet<_>>();
            if society_links != BTreeSet::from([society_id]) {
                return Err(CatalogError::Invalid(format!(
                    "property {property_id} must link to exactly society {society_id}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct CatalogStore {
    lake: LakeStore,
}

impl CatalogStore {
    pub fn new(lake: LakeStore) -> Self {
        Self { lake }
    }

    pub fn lake(&self) -> &LakeStore {
        &self.lake
    }

    pub async fn pointer(&self) -> Result<Option<CatalogPointer>, CatalogError> {
        match self.lake.get_json(&catalog_pointer_key()).await {
            Ok(pointer) => Ok(Some(pointer)),
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn current_roster(&self) -> Result<CatalogRoster, CatalogError> {
        let pointer = self
            .pointer()
            .await?
            .ok_or_else(|| CatalogError::Invalid("the dev catalog is not initialized".into()))?;
        self.read_roster(&pointer.current).await
    }

    pub async fn read_roster(
        &self,
        generation: &CatalogGeneration,
    ) -> Result<CatalogRoster, CatalogError> {
        let roster: CatalogRoster = self
            .lake
            .get_json(&LakeKey::new(generation.roster_key.clone())?)
            .await?;
        if roster.format_version != CATALOG_FORMAT_VERSION
            || roster.roster_id != generation.roster_id
        {
            return Err(CatalogError::Invalid(format!(
                "catalog roster {} does not match its pointer",
                generation.roster_key
            )));
        }
        roster.normalized_for_read()
    }

    pub async fn write_snapshot(
        &self,
        seed: SourceEntitySeed,
        records: &CatalogRecords,
        warnings: Vec<String>,
    ) -> Result<SocietyGoldSnapshotManifest, CatalogError> {
        validate_seed(&seed)?;
        if records.entities.is_empty() {
            return Err(CatalogError::Invalid(format!(
                "society {} produced an empty gold snapshot",
                seed.name
            )));
        }
        let snapshot_id = MaterializationId::new();
        let base = snapshot_data_prefix(&seed, &snapshot_id);
        let mut artifacts = Vec::new();
        artifacts.push(
            self.write_snapshot_artifact(
                CatalogArtifactKind::Entities,
                &base,
                "entities.parquet",
                write_entities_parquet(&records.entities)?,
                records.entities.len(),
            )
            .await?,
        );
        artifacts.push(
            self.write_snapshot_artifact(
                CatalogArtifactKind::Facts,
                &base,
                "facts.parquet",
                write_facts_parquet(&records.facts)?,
                records.facts.len(),
            )
            .await?,
        );
        artifacts.push(
            self.write_snapshot_artifact(
                CatalogArtifactKind::SearchMetadata,
                &base,
                "search_metadata.parquet",
                write_search_metadata_parquet(&records.search_metadata)?,
                records.search_metadata.len(),
            )
            .await?,
        );
        artifacts.push(
            self.write_snapshot_artifact(
                CatalogArtifactKind::Edges,
                &base,
                "edges.parquet",
                write_edges_parquet(&records.edges)?,
                records.edges.len(),
            )
            .await?,
        );
        artifacts.push(
            self.write_snapshot_artifact(
                CatalogArtifactKind::ReraEvidence,
                &base,
                "rera_evidence.parquet",
                write_rera_evidence_parquet(&records.rera_evidence)?,
                records.rera_evidence.len(),
            )
            .await?,
        );
        let manifest = SocietyGoldSnapshotManifest {
            format_version: GOLD_FORMAT_VERSION,
            snapshot_id: snapshot_id.clone(),
            society_id: canonical_seed_id(&seed).to_string(),
            seed,
            created_at: Utc::now(),
            entity_count: records.entities.len() as u64,
            fact_count: records.facts.len() as u64,
            search_metadata_count: records.search_metadata.len() as u64,
            edge_count: records.edges.len() as u64,
            rera_evidence_count: records.rera_evidence.len() as u64,
            artifacts,
            warnings,
        };
        self.lake
            .put_json(
                &snapshot_manifest_key(&manifest.seed, &snapshot_id),
                &manifest,
            )
            .await?;
        Ok(manifest)
    }

    pub async fn read_snapshot(
        &self,
        entry: &CatalogRosterEntry,
    ) -> Result<(SocietyGoldSnapshotManifest, CatalogRecords), CatalogError> {
        let key = LakeKey::new(entry.snapshot_manifest_key.clone())?;
        let manifest: SocietyGoldSnapshotManifest = self.lake.get_json(&key).await?;
        if manifest.format_version != GOLD_FORMAT_VERSION
            || manifest.snapshot_id != entry.snapshot_id
            || seed_identities(&manifest.seed).is_disjoint(&seed_identities(&entry.seed))
        {
            return Err(CatalogError::Invalid(format!(
                "society gold manifest {key} does not match roster entry"
            )));
        }
        let records = self
            .read_record_artifacts(
                &manifest.artifacts,
                [
                    manifest.entity_count,
                    manifest.fact_count,
                    manifest.search_metadata_count,
                    manifest.edge_count,
                    manifest.rera_evidence_count,
                ],
                &format!("society gold snapshot {}", manifest.snapshot_id),
            )
            .await?;
        Ok((manifest, records))
    }

    pub async fn read_topology_snapshot(
        &self,
        entry: &CatalogTopologyEntry,
    ) -> Result<(CatalogTopologySnapshotManifest, CatalogRecords), CatalogError> {
        let key = LakeKey::new(entry.snapshot_manifest_key.clone())?;
        let manifest: CatalogTopologySnapshotManifest = self.lake.get_json(&key).await?;
        if manifest.format_version != GOLD_FORMAT_VERSION
            || manifest.snapshot_id != entry.snapshot_id
        {
            return Err(CatalogError::Invalid(format!(
                "catalog topology manifest {key} does not match roster entry"
            )));
        }
        let records = self
            .read_record_artifacts(
                &manifest.artifacts,
                [
                    manifest.entity_count,
                    manifest.fact_count,
                    manifest.search_metadata_count,
                    manifest.edge_count,
                    manifest.rera_evidence_count,
                ],
                &format!("catalog topology snapshot {}", manifest.snapshot_id),
            )
            .await?;
        Ok((manifest, records))
    }

    pub async fn assemble_and_promote(
        &self,
        operation: impl Into<String>,
        roster: CatalogRoster,
        expected: Option<&CatalogPointer>,
    ) -> Result<CatalogOperationReport, CatalogError> {
        let operation = operation.into();
        let roster = roster.normalized()?;
        if roster.societies.is_empty() {
            return Err(CatalogError::Invalid(
                "refusing to publish an empty serving catalog".to_string(),
            ));
        }
        let mut warnings = Vec::new();
        let mut snapshots = Vec::new();
        if let Some(topology) = &roster.topology {
            let (_, records) = self.read_topology_snapshot(topology).await?;
            snapshots.push(records);
        }
        for entry in &roster.societies {
            let (manifest, records) = self.read_snapshot(entry).await?;
            warnings.extend(
                manifest
                    .warnings
                    .into_iter()
                    .map(|warning| format!("{}: {warning}", entry.seed.name)),
            );
            snapshots.push(records);
        }
        let merged = merge_catalog_records(snapshots)?;
        validate_catalog_records(&merged, &roster)?;

        let bundle_version = format!(
            "catalog-{}-{}",
            roster.societies.len(),
            MaterializationId::new()
        );
        let manifest = ServingBundleBuilder::new(self.lake.clone())
            .build_from_catalog_records(
                merged.entities,
                merged.facts,
                merged.search_metadata,
                merged.edges,
                merged.rera_evidence,
                bundle_version.clone(),
            )
            .await?;
        if manifest.format_version != SERVING_BUNDLE_FORMAT_VERSION {
            return Err(CatalogError::Invalid(format!(
                "serving builder emitted format {}, expected {SERVING_BUNDLE_FORMAT_VERSION}",
                manifest.format_version
            )));
        }
        self.validate_serving_generation(&bundle_version).await?;
        if manifest.quarantined_society_count > 0 {
            warnings.push(format!(
                "{} societies were quarantined by serving eligibility",
                manifest.quarantined_society_count
            ));
        }
        if !manifest.excluded_rera_evidence_society_ids.is_empty() {
            warnings.push(format!(
                "RERA evidence for {} out-of-catalog societies was omitted",
                manifest.excluded_rera_evidence_society_ids.len()
            ));
        }

        let roster_key = roster_key(&roster.roster_id);
        self.lake.put_json(&roster_key, &roster).await?;
        let generation = CatalogGeneration {
            roster_id: roster.roster_id.clone(),
            roster_key: roster_key.to_string(),
            bundle_version: bundle_version.clone(),
        };
        let pointer = CatalogPointer {
            format_version: CATALOG_FORMAT_VERSION,
            revision: expected.map_or(1, |pointer| pointer.revision + 1),
            current: generation,
            previous: expected.map(|pointer| pointer.current.clone()),
            updated_at: Utc::now(),
        };
        let promoted = self
            .lake
            .put_json_if(
                &catalog_pointer_key(),
                &pointer,
                |current: Option<&CatalogPointer>| {
                    current.map(|value| value.revision) == expected.map(|value| value.revision)
                        && current.map(|value| &value.current)
                            == expected.map(|value| &value.current)
                },
            )
            .await?;
        if !promoted {
            return Err(CatalogError::CasConflict);
        }
        if let Err(error) = self.gc(&pointer).await {
            warnings.push(format!("catalog cleanup deferred: {error}"));
        }
        Ok(CatalogOperationReport {
            operation,
            revision: pointer.revision,
            society_count: roster.societies.len(),
            bundle_version,
            warnings,
        })
    }

    pub async fn upsert_snapshot(
        &self,
        seed: SourceEntitySeed,
        records: &CatalogRecords,
        warnings: Vec<String>,
    ) -> Result<CatalogOperationReport, CatalogError> {
        let expected = self.pointer().await?;
        let mut roster = match &expected {
            Some(pointer) => self.read_roster(&pointer.current).await?,
            None => CatalogRoster::empty(),
        };
        let snapshot = self.write_snapshot(seed.clone(), records, warnings).await?;
        let identities = seed_identities(&seed);
        roster
            .societies
            .retain(|entry| seed_identities(&entry.seed).is_disjoint(&identities));
        roster.societies.push(CatalogRosterEntry {
            seed,
            snapshot_id: snapshot.snapshot_id.clone(),
            snapshot_manifest_key: snapshot_manifest_key(&snapshot.seed, &snapshot.snapshot_id)
                .to_string(),
        });
        self.assemble_and_promote("add", roster, expected.as_ref())
            .await
    }

    pub async fn remove(&self, society_id: &str) -> Result<CatalogOperationReport, CatalogError> {
        let expected = self
            .pointer()
            .await?
            .ok_or_else(|| CatalogError::Invalid("the dev catalog is not initialized".into()))?;
        let mut roster = self.read_roster(&expected.current).await?;
        let identity = normalize_identity(society_id);
        let before = roster.societies.len();
        roster
            .societies
            .retain(|entry| !seed_identities(&entry.seed).contains(&identity));
        if roster.societies.len() == before {
            return Err(CatalogError::Invalid(format!(
                "society {society_id} is not in the active roster"
            )));
        }
        self.assemble_and_promote("remove", roster, Some(&expected))
            .await
    }

    pub async fn undo(&self) -> Result<CatalogOperationReport, CatalogError> {
        let expected = self
            .pointer()
            .await?
            .ok_or_else(|| CatalogError::Invalid("the dev catalog is not initialized".into()))?;
        let previous = expected.previous.clone().ok_or_else(|| {
            CatalogError::Invalid("the dev catalog has no previous bundle to restore".into())
        })?;
        let roster = self.read_roster(&previous).await?;
        self.validate_serving_generation(&previous.bundle_version)
            .await?;
        let pointer = CatalogPointer {
            format_version: CATALOG_FORMAT_VERSION,
            revision: expected.revision + 1,
            current: previous,
            previous: Some(expected.current.clone()),
            updated_at: Utc::now(),
        };
        let promoted = self
            .lake
            .put_json_if(
                &catalog_pointer_key(),
                &pointer,
                |current: Option<&CatalogPointer>| current == Some(&expected),
            )
            .await?;
        if !promoted {
            return Err(CatalogError::CasConflict);
        }
        let warnings = self
            .gc(&pointer)
            .await
            .err()
            .map(|error| vec![format!("catalog cleanup deferred: {error}")])
            .unwrap_or_default();
        Ok(CatalogOperationReport {
            operation: "undo".to_string(),
            revision: pointer.revision,
            society_count: roster.societies.len(),
            bundle_version: pointer.current.bundle_version.clone(),
            warnings,
        })
    }

    async fn validate_serving_generation(&self, bundle_version: &str) -> Result<(), CatalogError> {
        let validation = validate_search_serving_candidate(&self.lake, bundle_version).await?;
        if !validation.passed {
            return Err(CatalogError::Invalid(format!(
                "serving validation failed: {}",
                validation
                    .issues
                    .iter()
                    .map(|issue| issue.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        if validation.property_count == 0 {
            return Err(CatalogError::Invalid(
                "refusing to publish a serving catalog with no properties".into(),
            ));
        }
        ServingBundleLoader::new(
            self.lake.clone(),
            std::env::temp_dir().join("openestates-catalog-validation"),
        )
        .load_search_bundle(bundle_version)
        .await?;
        Ok(())
    }

    async fn write_snapshot_artifact(
        &self,
        kind: CatalogArtifactKind,
        base: &str,
        name: &str,
        bytes: Vec<u8>,
        row_count: usize,
    ) -> Result<CatalogArtifact, CatalogError> {
        let key = LakeKey::join(&[base, name])?;
        let metadata = self.lake.put_bytes(&key, bytes).await?;
        Ok(catalog_artifact(kind, metadata, row_count))
    }

    async fn read_record_artifacts(
        &self,
        artifacts: &[CatalogArtifact],
        expected_counts: [u64; 5],
        label: &str,
    ) -> Result<CatalogRecords, CatalogError> {
        let mut records = CatalogRecords::default();
        let mut seen = BTreeSet::new();
        for artifact in artifacts {
            if !seen.insert(artifact.kind) {
                return Err(CatalogError::Invalid(format!(
                    "{label} repeats the {:?} artifact",
                    artifact.kind
                )));
            }
            let key = LakeKey::new(artifact.key.clone())?;
            self.lake
                .verify_artifact(&key, artifact.size_bytes, &artifact.content_hash)
                .await?;
            let bytes = self.lake.get_bytes(&key).await?;
            match artifact.kind {
                CatalogArtifactKind::Entities => records.entities = read_entities_parquet(&bytes)?,
                CatalogArtifactKind::Facts => records.facts = read_facts_parquet(&bytes)?,
                CatalogArtifactKind::SearchMetadata => {
                    records.search_metadata = read_search_metadata_parquet(&bytes)?
                }
                CatalogArtifactKind::Edges => records.edges = read_edges_parquet(&bytes)?,
                CatalogArtifactKind::ReraEvidence => {
                    records.rera_evidence = read_rera_evidence_parquet(&bytes)?
                }
            }
        }
        if seen.len() != 5
            || [
                records.entities.len() as u64,
                records.facts.len() as u64,
                records.search_metadata.len() as u64,
                records.edges.len() as u64,
                records.rera_evidence.len() as u64,
            ] != expected_counts
        {
            return Err(CatalogError::Invalid(format!(
                "{label} artifacts or row counts do not match its manifest"
            )));
        }
        Ok(records)
    }

    async fn gc(&self, pointer: &CatalogPointer) -> Result<(), CatalogError> {
        let generations = [Some(&pointer.current), pointer.previous.as_ref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let retained_roster_keys = generations
            .iter()
            .map(|generation| generation.roster_key.as_str())
            .collect::<HashSet<_>>();
        let retained_bundle_prefixes = generations
            .iter()
            .map(|generation| {
                format!(
                    "serving/search_bundle/version={}/",
                    generation.bundle_version
                )
            })
            .collect::<Vec<_>>();
        let mut retained_snapshot_keys = HashSet::new();
        for generation in &generations {
            let roster = self.read_roster(generation).await?;
            retained_snapshot_keys.extend(
                roster
                    .societies
                    .iter()
                    .map(|entry| entry.snapshot_manifest_key.clone()),
            );
            if let Some(topology) = roster.topology {
                retained_snapshot_keys.insert(topology.snapshot_manifest_key);
            }
        }

        // Delete namespaces written by the retired serving-materialization and
        // release workflows. These keys are cleanup targets, never read paths.
        let prefix = LakePrefix::new("manifests/assets/search_serving_bundle")?;
        for key in self.lake.list_keys(&prefix).await? {
            self.lake.delete(&key).await?;
        }
        for key in self
            .lake
            .list_keys(&LakePrefix::new("manifests/materializations")?)
            .await?
        {
            let lookup: serde_json::Value = self.lake.get_json(&key).await?;
            if lookup.get("asset_id").and_then(|value| value.as_str())
                == Some("search_serving_bundle")
            {
                self.lake.delete(&key).await?;
            }
        }

        for key in self
            .lake
            .list_keys(&LakePrefix::new("serving/search_bundle")?)
            .await?
        {
            if !retained_bundle_prefixes
                .iter()
                .any(|prefix| key.as_str().starts_with(prefix))
            {
                self.lake.delete(&key).await?;
            }
        }

        for prefix in [
            LakePrefix::new("manifests/catalog/releases")?,
            LakePrefix::new("manifests/catalog/environments")?,
        ] {
            for key in self.lake.list_keys(&prefix).await? {
                self.lake.delete(&key).await?;
            }
        }

        let roster_prefix = LakePrefix::new("manifests/catalog/rosters")?;
        for key in self.lake.list_keys(&roster_prefix).await? {
            if !retained_roster_keys.contains(key.as_str()) {
                self.lake.delete(&key).await?;
            }
        }
        for prefix in [
            LakePrefix::new("manifests/catalog/societies")?,
            LakePrefix::new("manifests/catalog/topology")?,
        ] {
            for key in self.lake.list_keys(&prefix).await? {
                if retained_snapshot_keys.contains(key.as_str()) {
                    continue;
                }
                let manifest: serde_json::Value = self.lake.get_json(&key).await?;
                if let Some(artifacts) =
                    manifest.get("artifacts").and_then(|value| value.as_array())
                {
                    for artifact in artifacts {
                        if let Some(artifact_key) =
                            artifact.get("key").and_then(|value| value.as_str())
                        {
                            self.lake
                                .delete(&LakeKey::new(artifact_key.to_string())?)
                                .await?;
                        }
                    }
                }
                self.lake.delete(&key).await?;
            }
        }
        Ok(())
    }
}

/// Run the normal scoped DAG for exactly one society and project its immutable
/// gold output into catalog rows. No temporary serving bundle is built, so the
/// expensive source collection is isolated to the requested society and
/// runtime state is not touched.
pub async fn collect_society_from_dag(
    store: &CatalogStore,
    project_root: &Path,
    seed: &SourceEntitySeed,
) -> Result<(CatalogRecords, Vec<String>), CatalogError> {
    validate_seed(seed)?;
    let planned_at = Utc::now();
    let source_assets = AssetSourceInputs::supported_asset_ids();
    let request = SourceInputRequest {
        project_root: project_root.to_path_buf(),
        partition: AssetPartition::new([("society", safe_segment(canonical_seed_id(seed)))]),
        planned_at,
        requested_assets: source_assets.clone(),
        force_refresh_assets: source_assets.clone(),
        source_entities: vec![seed.clone()],
    };
    let python =
        std::env::var("OPENESTATES_SOURCE_PYTHON").unwrap_or_else(|_| "python3.11".to_string());
    let provider = CommandSourceInputProvider::new(python).with_args([
        OsString::from("-m"),
        OsString::from("pipeline.collect_asset_sources"),
    ]);
    let mut source_inputs = provider
        .load(&request, store.lake())
        .await
        .map_err(|error| CatalogError::Invalid(format!("society collection failed: {error}")))?
        .ok_or_else(|| CatalogError::Invalid("society collector returned no inputs".into()))?;
    source_inputs.source_entities = vec![seed.clone()];
    let mut warnings = source_inputs
        .source_failures
        .iter()
        .map(|(source, message)| format!("{source}: {message}"))
        .collect::<Vec<_>>();

    let registry = openestates_registry();
    let forced_assets = registry
        .definitions()
        .iter()
        .map(|definition| definition.id.clone())
        .collect();
    let version = format!(
        "society-{}-{}",
        safe_segment(canonical_seed_id(seed)),
        MaterializationId::new()
    );
    let options = AssetDagExecutionOptions::new(
        AssetPartition::new([("society", safe_segment(canonical_seed_id(seed)))]),
        planned_at,
    )
    .with_version(version)
    .with_source_scope(SourceEntityResolutionScope::Scoped)
    .with_source_inputs(source_inputs)
    .with_skip_missing_source_inputs(true)
    .with_forced_assets(forced_assets)
    .with_only_forced_assets(true);
    let report = AssetDagExecutor::new(registry, store.lake().clone())
        .execute(&KnowledgeGraph::new(), options)
        .await
        .map_err(|error| CatalogError::Invalid(format!("society DAG failed: {error}")))?;
    warnings.extend(report.manifest.steps.iter().filter_map(|step| {
        (step.status != AssetRunStepStatus::Succeeded)
            .then(|| {
                step.error
                    .as_ref()
                    .map(|error| format!("{}: {error}", step.asset_id))
            })
            .flatten()
    }));
    let gold_step = report
        .manifest
        .steps
        .iter()
        .find(|step| step.asset_id.as_str() == SOCIETY_GOLD_SNAPSHOT_ASSET_ID)
        .ok_or_else(|| CatalogError::Invalid("society DAG produced no gold snapshot".into()))?;
    let gold_manifest_key = gold_step
        .artifacts
        .iter()
        .find(|artifact| artifact.key.ends_with("/manifest.json"))
        .map(|artifact| LakeKey::new(artifact.key.clone()))
        .transpose()?
        .ok_or_else(|| CatalogError::Invalid("society gold manifest is missing".into()))?;
    let gold_manifest: SocietyGoldManifest = store.lake().get_json(&gold_manifest_key).await?;
    let gold = load_society_gold_records(store.lake(), &gold_manifest)
        .await
        .map_err(|error| CatalogError::Invalid(format!("society gold is corrupt: {error}")))?;
    let materializations = AssetMaterializationStore::new(store.lake().clone());
    let mut rera_records = BTreeMap::new();
    for asset_name in [
        RERA_RECEIPTS_ASSET_ID,
        RERA_SOURCE_RECORDS_ASSET_ID,
        RERA_CLAIMS_ASSET_ID,
    ] {
        let asset_id = AssetId::new(asset_name).expect("static asset id");
        let record = report
            .manifest
            .steps
            .iter()
            .find(|step| step.asset_id == asset_id)
            .and_then(|step| step.materialization_id.as_ref());
        if let Some(materialization_id) = record {
            if let Some(record) = materializations
                .record_by_id_for_asset(&asset_id, materialization_id)
                .await?
            {
                rera_records.insert(asset_name, record);
            }
        }
    }
    let rera_evidence = if rera_records.len() == 3 {
        let receipts = read_rera_receipt_records(
            store.lake(),
            rera_records.get(RERA_RECEIPTS_ASSET_ID).expect("checked"),
        )
        .await
        .map_err(|error| CatalogError::Invalid(format!("RERA receipts are corrupt: {error}")))?;
        let sources = read_rera_source_records(
            store.lake(),
            rera_records
                .get(RERA_SOURCE_RECORDS_ASSET_ID)
                .expect("checked"),
        )
        .await
        .map_err(|error| CatalogError::Invalid(format!("RERA sources are corrupt: {error}")))?;
        let claims = read_rera_claims(
            store.lake(),
            rera_records.get(RERA_CLAIMS_ASSET_ID).expect("checked"),
        )
        .await
        .map_err(|error| CatalogError::Invalid(format!("RERA claims are corrupt: {error}")))?;
        project_rera_evidence(&sources, &claims, &receipts)
            .map_err(|error| CatalogError::Invalid(format!("RERA evidence is invalid: {error}")))?
    } else {
        warnings.push("RERA evidence was unavailable for this collection".to_string());
        Vec::new()
    };
    let records = CatalogRecords::from_society_gold(&gold, rera_evidence)?.for_society(seed)?;
    warnings.sort();
    warnings.dedup();
    Ok((records, warnings))
}

impl CatalogRoster {
    fn normalized_for_read(mut self) -> Result<Self, CatalogError> {
        if self.format_version != CATALOG_FORMAT_VERSION {
            return Err(CatalogError::Invalid(format!(
                "unsupported catalog roster format {}",
                self.format_version
            )));
        }
        self.societies.sort_by(|left, right| {
            canonical_seed_id(&left.seed).cmp(canonical_seed_id(&right.seed))
        });
        let mut identities = HashSet::new();
        for entry in &self.societies {
            validate_seed(&entry.seed)?;
            for identity in seed_identities(&entry.seed) {
                if !identities.insert(identity.clone()) {
                    return Err(CatalogError::Invalid(format!(
                        "duplicate canonical catalog identity {identity}"
                    )));
                }
            }
        }
        Ok(self)
    }
}

fn merge_catalog_records(snapshots: Vec<CatalogRecords>) -> Result<CatalogRecords, CatalogError> {
    let mut entities = BTreeMap::<String, ServingEntityRecord>::new();
    let mut facts = BTreeMap::<String, ServingFactRecord>::new();
    let mut search_metadata = BTreeMap::<String, ServingSearchMetadataRecord>::new();
    let mut edges = BTreeMap::<String, ServingEdgeRecord>::new();
    let mut rera_evidence = BTreeMap::<String, ServingReraEvidenceRecord>::new();
    for snapshot in snapshots {
        for entity in snapshot.entities {
            merge_record(
                &mut entities,
                entity.entity_id.clone(),
                entity,
                "entity identity",
            )?;
        }
        for fact in snapshot.facts {
            let key = serde_json::to_string(&fact)?;
            facts.entry(key).or_insert(fact);
        }
        for row in snapshot.search_metadata {
            let key = serde_json::to_string(&row)?;
            search_metadata.entry(key).or_insert(row);
        }
        for edge in snapshot.edges {
            let key = format!(
                "{}\0{}\0{}",
                edge.from_entity_id, edge.edge_type, edge.to_entity_id
            );
            merge_record(&mut edges, key, edge, "relation")?;
        }
        for record in snapshot.rera_evidence {
            merge_record(
                &mut rera_evidence,
                record.society_id.clone(),
                record,
                "RERA evidence",
            )?;
        }
    }
    Ok(CatalogRecords {
        entities: entities.into_values().collect(),
        facts: facts.into_values().collect(),
        search_metadata: search_metadata.into_values().collect(),
        edges: edges.into_values().collect(),
        rera_evidence: rera_evidence.into_values().collect(),
    })
}

fn merge_record<T: PartialEq>(
    records: &mut BTreeMap<String, T>,
    key: String,
    record: T,
    kind: &str,
) -> Result<(), CatalogError> {
    if let Some(existing) = records.get(&key) {
        if existing != &record {
            return Err(CatalogError::Invalid(format!(
                "contradictory {kind} rows for {key}"
            )));
        }
    } else {
        records.insert(key, record);
    }
    Ok(())
}

fn validate_catalog_records(
    records: &CatalogRecords,
    roster: &CatalogRoster,
) -> Result<(), CatalogError> {
    if records.entities.is_empty() {
        return Err(CatalogError::Invalid("empty serving catalog".into()));
    }
    let entity_ids = records
        .entities
        .iter()
        .map(|entity| entity.entity_id.as_str())
        .collect::<HashSet<_>>();
    let society_ids = records
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "society")
        .map(|entity| entity.entity_id.as_str())
        .collect::<HashSet<_>>();
    for edge in &records.edges {
        if !entity_ids.contains(edge.from_entity_id.as_str())
            || !entity_ids.contains(edge.to_entity_id.as_str())
        {
            return Err(CatalogError::Invalid(format!(
                "dangling relation {} -[{}]-> {}",
                edge.from_entity_id, edge.edge_type, edge.to_entity_id
            )));
        }
    }
    for fact in &records.facts {
        if !entity_ids.contains(fact.entity_id.as_str()) {
            return Err(CatalogError::Invalid(format!(
                "fact {}/{} names a missing entity",
                fact.entity_id, fact.fact_key
            )));
        }
    }
    for row in &records.search_metadata {
        if !entity_ids.contains(row.entity_id.as_str()) {
            return Err(CatalogError::Invalid(format!(
                "search metadata {}/{} names a missing entity",
                row.entity_id, row.fact_key
            )));
        }
    }
    for property in records
        .entities
        .iter()
        .filter(|entity| entity.entity_type == "property")
    {
        let links = records
            .edges
            .iter()
            .filter(|edge| {
                edge.edge_type == "in_society" && edge.from_entity_id == property.entity_id
            })
            .map(|edge| edge.to_entity_id.as_str())
            .collect::<BTreeSet<_>>();
        if links.len() != 1 {
            return Err(CatalogError::Invalid(format!(
                "property {} must link to exactly one society, found {}",
                property.entity_id,
                links.len()
            )));
        }
        if links
            .iter()
            .any(|society_id| !society_ids.contains(society_id))
        {
            return Err(CatalogError::Invalid(format!(
                "property {} links to a non-society entity",
                property.entity_id
            )));
        }
    }
    let mut evidence_owner_by_identity = BTreeMap::new();
    for (index, entry) in roster.societies.iter().enumerate() {
        for identity in seed_identities(&entry.seed) {
            evidence_owner_by_identity.insert(identity, index);
        }
    }
    let mut evidence_owners = HashSet::new();
    for evidence in &records.rera_evidence {
        let identity = normalize_identity(&evidence.society_id);
        let Some(owner) = evidence_owner_by_identity.get(&identity) else {
            return Err(CatalogError::Invalid(format!(
                "RERA evidence names out-of-catalog society {}",
                evidence.society_id
            )));
        };
        if !evidence_owners.insert(*owner) {
            return Err(CatalogError::Invalid(format!(
                "catalog contains contradictory RERA evidence identities for {}",
                roster.societies[*owner].seed.name
            )));
        }
    }
    Ok(())
}

fn catalog_artifact(
    kind: CatalogArtifactKind,
    metadata: ArtifactMetadata,
    row_count: usize,
) -> CatalogArtifact {
    CatalogArtifact {
        kind,
        key: metadata.key.to_string(),
        content_hash: metadata.content_hash,
        size_bytes: metadata.size_bytes,
        row_count: row_count as u64,
    }
}

fn validate_seed(seed: &SourceEntitySeed) -> Result<(), CatalogError> {
    if seed.entity_id.trim().is_empty() || seed.name.trim().is_empty() {
        return Err(CatalogError::Invalid(
            "society seed requires entity_id and name".into(),
        ));
    }
    match (seed.latitude, seed.longitude) {
        (Some(latitude), Some(longitude))
            if latitude.is_finite()
                && longitude.is_finite()
                && (-90.0..=90.0).contains(&latitude)
                && (-180.0..=180.0).contains(&longitude) => {}
        (None, None) => {}
        _ => {
            return Err(CatalogError::Invalid(format!(
                "society {} has invalid or incomplete coordinates",
                seed.name
            )))
        }
    }
    Ok(())
}

fn canonical_seed_id(seed: &SourceEntitySeed) -> &str {
    seed.alias_entity_id.as_deref().unwrap_or(&seed.entity_id)
}

fn seed_identities(seed: &SourceEntitySeed) -> HashSet<String> {
    [
        Some(seed.entity_id.as_str()),
        seed.alias_entity_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(normalize_identity)
    .collect()
}

fn normalize_identity(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    normalized
        .strip_prefix("society:")
        .or_else(|| normalized.strip_prefix("soc-"))
        .unwrap_or(&normalized)
        .to_string()
}

fn safe_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub fn catalog_pointer_key() -> LakeKey {
    LakeKey::new("manifests/catalog/dev.json").expect("static catalog pointer key")
}

fn roster_key(roster_id: &MaterializationId) -> LakeKey {
    LakeKey::new(format!("manifests/catalog/rosters/{roster_id}.json"))
        .expect("roster id makes a valid key")
}

fn snapshot_data_prefix(seed: &SourceEntitySeed, snapshot_id: &MaterializationId) -> String {
    format!(
        "gold/society_catalog/society={}/snapshot={snapshot_id}",
        safe_segment(canonical_seed_id(seed))
    )
}

fn snapshot_manifest_key(seed: &SourceEntitySeed, snapshot_id: &MaterializationId) -> LakeKey {
    LakeKey::new(format!(
        "manifests/catalog/societies/society={}/snapshots/{snapshot_id}.json",
        safe_segment(canonical_seed_id(seed))
    ))
    .expect("society snapshot key")
}

#[derive(Debug)]
pub enum CatalogError {
    CasConflict,
    Invalid(String),
    Lake(LakeError),
    Key(crate::lake::keys::KeyError),
    ParquetRead(crate::serving::ParquetReadError),
    ParquetWrite(crate::serving::ParquetWriteError),
    ServingBuild(ServingBundleError),
    ServingLoad(ServingBundleLoadError),
    ServingValidation(crate::serving::ServingBundleValidationError),
    Json(serde_json::Error),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CasConflict => {
                formatter.write_str("catalog changed concurrently; retry the operation")
            }
            Self::Invalid(message) => write!(formatter, "invalid catalog: {message}"),
            Self::Lake(error) => write!(formatter, "catalog storage failed: {error}"),
            Self::Key(error) => write!(formatter, "catalog key failed: {error}"),
            Self::ParquetRead(error) => write!(formatter, "catalog parquet read failed: {error}"),
            Self::ParquetWrite(error) => write!(formatter, "catalog parquet write failed: {error}"),
            Self::ServingBuild(error) => write!(formatter, "catalog serving build failed: {error}"),
            Self::ServingLoad(error) => write!(formatter, "catalog serving load failed: {error}"),
            Self::ServingValidation(error) => {
                write!(formatter, "catalog serving validation failed: {error}")
            }
            Self::Json(error) => write!(formatter, "catalog JSON failed: {error}"),
        }
    }
}

impl std::error::Error for CatalogError {}

macro_rules! catalog_from {
    ($variant:ident, $source:ty) => {
        impl From<$source> for CatalogError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

catalog_from!(Lake, LakeError);
catalog_from!(Key, crate::lake::keys::KeyError);
catalog_from!(ParquetRead, crate::serving::ParquetReadError);
catalog_from!(ParquetWrite, crate::serving::ParquetWriteError);
catalog_from!(ServingBuild, ServingBundleError);
catalog_from!(ServingLoad, ServingBundleLoadError);
catalog_from!(
    ServingValidation,
    crate::serving::ServingBundleValidationError
);
catalog_from!(Json, serde_json::Error);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::FactValue;
    use crate::serving::ServingEntityVisibility;
    use tempfile::tempdir;

    #[tokio::test]
    async fn add_replace_remove_and_undo_are_atomic_catalog_generations() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogStore::new(lake.clone());

        let alpha = seed("alpha");
        let first = store
            .upsert_snapshot(
                alpha.clone(),
                &records("alpha", "Alpha", 1_000.0),
                vec!["groundwater evidence unavailable".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.society_count, 1);
        assert_eq!(first.warnings.len(), 1);
        let mut legacy_pointer = serde_json::to_value(store.pointer().await.unwrap()).unwrap();
        legacy_pointer["current"]["serving_materialization_id"] = serde_json::json!("retired");
        assert!(serde_json::from_value::<Option<CatalogPointer>>(legacy_pointer).is_err());

        let beta = seed("beta");
        let second = store
            .upsert_snapshot(beta, &records("beta", "Beta", 1_100.0), Vec::new())
            .await
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.society_count, 2);
        let second_pointer = store.pointer().await.unwrap().unwrap();
        let second_bundle =
            ServingBundleLoader::new(lake.clone(), root.path().join("cache-second"))
                .load_search_bundle(&second_pointer.current.bundle_version)
                .await
                .unwrap();
        assert_eq!(
            second_bundle
                .entities
                .iter()
                .filter(|entity| entity.entity_id.contains("alpha"))
                .map(|entity| entity.entity_id.as_str())
                .collect::<Vec<_>>(),
            ["property:alpha", "society:alpha"]
        );

        let replacement = store
            .upsert_snapshot(alpha, &records("alpha", "Alpha", 1_250.0), Vec::new())
            .await
            .unwrap();
        assert_eq!(replacement.revision, 3);
        assert_eq!(replacement.society_count, 2);
        let roster = store.current_roster().await.unwrap();
        assert_eq!(roster.societies.len(), 2);
        let replacement_pointer = store.pointer().await.unwrap().unwrap();
        let replacement_bundle =
            ServingBundleLoader::new(lake.clone(), root.path().join("cache-replacement"))
                .load_search_bundle(&replacement_pointer.current.bundle_version)
                .await
                .unwrap();
        let alpha_sizes = replacement_bundle
            .fact_index
            .entity("property:alpha")
            .unwrap()
            .facts
            .iter()
            .filter(|fact| fact.fact_key == "carpet_area_sqft")
            .filter_map(|fact| match fact.value {
                FactValue::Numeric(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(alpha_sizes, [1_250.0]);

        let removed = store.remove("society:beta").await.unwrap();
        assert_eq!(removed.revision, 4);
        assert_eq!(removed.society_count, 1);
        let pointer_before_undo = store.pointer().await.unwrap().unwrap();
        let removed_bundle =
            ServingBundleLoader::new(lake.clone(), root.path().join("cache-removed"))
                .load_search_bundle(&pointer_before_undo.current.bundle_version)
                .await
                .unwrap();
        assert!(removed_bundle
            .entities
            .iter()
            .all(|entity| !entity.entity_id.contains("beta")));
        assert!(removed_bundle
            .fact_index
            .all_facts()
            .iter()
            .all(|fact| !fact.entity_id.contains("beta")));
        assert!(removed_bundle.edges.iter().all(|edge| {
            !edge.from_entity_id.contains("beta") && !edge.to_entity_id.contains("beta")
        }));
        assert!(removed_bundle
            .rera_evidence_index
            .society("society:beta")
            .is_none());
        let undone = store.undo().await.unwrap();
        assert_eq!(undone.revision, 5);
        assert_eq!(undone.society_count, 2);
        let pointer_after_undo = store.pointer().await.unwrap().unwrap();
        assert_eq!(
            pointer_after_undo.current.bundle_version,
            pointer_before_undo.previous.unwrap().bundle_version
        );

        let loaded = ServingBundleLoader::new(lake.clone(), root.path().join("cache"))
            .load_search_bundle(&pointer_after_undo.current.bundle_version)
            .await
            .unwrap();
        let properties = loaded
            .entities
            .iter()
            .filter(|entity| entity.entity_type == "property")
            .map(|entity| entity.entity_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            properties,
            BTreeSet::from(["property:alpha", "property:beta"])
        );

        let retained_bundle_manifests = lake
            .list_keys(&LakePrefix::new("serving/search_bundle").unwrap())
            .await
            .unwrap()
            .into_iter()
            .filter(|key| key.as_str().ends_with("/manifest.json"))
            .count();
        assert_eq!(retained_bundle_manifests, 2);
        assert_eq!(
            lake.list_keys(&LakePrefix::new("manifests/catalog/rosters").unwrap())
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn corrupt_snapshot_and_stale_cas_leave_dev_unchanged() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogStore::new(lake.clone());
        store
            .upsert_snapshot(seed("alpha"), &records("alpha", "Alpha", 900.0), Vec::new())
            .await
            .unwrap();
        let stable = store.pointer().await.unwrap().unwrap();
        let roster = store.current_roster().await.unwrap();
        let manifest: SocietyGoldSnapshotManifest = lake
            .get_json(&LakeKey::new(roster.societies[0].snapshot_manifest_key.clone()).unwrap())
            .await
            .unwrap();
        let facts = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == CatalogArtifactKind::Facts)
            .unwrap();
        lake.put_text(&LakeKey::new(facts.key.clone()).unwrap(), "corrupt")
            .await
            .unwrap();
        let error = store
            .assemble_and_promote("rebuild", roster.clone(), Some(&stable))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("manifest"));
        assert_eq!(store.pointer().await.unwrap().unwrap(), stable);

        let fresh_root = tempdir().unwrap();
        let fresh_lake = LakeStore::local(fresh_root.path()).unwrap();
        let fresh_store = CatalogStore::new(fresh_lake);
        fresh_store
            .upsert_snapshot(seed("alpha"), &records("alpha", "Alpha", 900.0), Vec::new())
            .await
            .unwrap();
        let stale = fresh_store.pointer().await.unwrap().unwrap();
        fresh_store
            .upsert_snapshot(seed("beta"), &records("beta", "Beta", 900.0), Vec::new())
            .await
            .unwrap();
        let current_roster = fresh_store.current_roster().await.unwrap();
        let error = fresh_store
            .assemble_and_promote("race", current_roster, Some(&stale))
            .await
            .unwrap_err();
        assert!(matches!(error, CatalogError::CasConflict));
        assert_eq!(fresh_store.pointer().await.unwrap().unwrap().revision, 2);
    }

    #[tokio::test]
    async fn undo_validates_the_previous_bundle_before_swapping_dev() {
        let root = tempdir().unwrap();
        let lake = LakeStore::local(root.path()).unwrap();
        let store = CatalogStore::new(lake.clone());
        store
            .upsert_snapshot(seed("alpha"), &records("alpha", "Alpha", 900.0), Vec::new())
            .await
            .unwrap();
        store
            .upsert_snapshot(seed("beta"), &records("beta", "Beta", 900.0), Vec::new())
            .await
            .unwrap();
        let stable = store.pointer().await.unwrap().unwrap();
        let previous = stable.previous.as_ref().unwrap();
        let manifest_key = crate::assets::AssetPathBuilder::serving_bundle_key(
            &previous.bundle_version,
            "manifest.json",
        );
        let manifest: ServingBundleManifest = lake.get_json(&manifest_key).await.unwrap();
        lake.put_text(
            &LakeKey::new(manifest.schema_key).unwrap(),
            "corrupt schema",
        )
        .await
        .unwrap();

        assert!(store.undo().await.is_err());
        assert_eq!(store.pointer().await.unwrap().unwrap(), stable);
    }

    #[test]
    fn rera_and_runtime_society_ids_share_one_roster_identity() {
        let seed = SourceEntitySeed {
            entity_id: "society:rera-526d82c4b10990fe".to_string(),
            alias_entity_id: Some("society:amrutha-platinum-towers".to_string()),
            name: "Amrutha Platinum Towers".to_string(),
            area: None,
            city: Some("Bengaluru".to_string()),
            project_key: None,
            latitude: None,
            longitude: None,
        };
        assert_eq!(
            normalize_identity("soc-amrutha-platinum-towers"),
            "amrutha-platinum-towers"
        );
        assert_eq!(
            seed_identities(&seed),
            HashSet::from([
                "rera-526d82c4b10990fe".to_string(),
                "amrutha-platinum-towers".to_string(),
            ])
        );
    }

    fn seed(slug: &str) -> SourceEntitySeed {
        SourceEntitySeed {
            entity_id: format!("society:{slug}"),
            alias_entity_id: None,
            name: slug.to_ascii_uppercase(),
            area: Some("Whitefield".to_string()),
            city: Some("Bengaluru".to_string()),
            project_key: Some(format!("RERA-{slug}")),
            latitude: None,
            longitude: None,
        }
    }

    fn records(slug: &str, name: &str, size: f64) -> CatalogRecords {
        let society_id = format!("society:{slug}");
        let property_id = format!("property:{slug}");
        let learned_at = Utc::now();
        let entities = vec![
            ServingEntityRecord {
                entity_id: society_id.clone(),
                entity_type: "society".to_string(),
                name: name.to_string(),
                root_source: Some("rera".to_string()),
                visibility: ServingEntityVisibility::Searchable,
                searchable_text: name.to_string(),
            },
            ServingEntityRecord {
                entity_id: property_id.clone(),
                entity_type: "property".to_string(),
                name: format!("{name} 3 BHK"),
                root_source: Some("catalog".to_string()),
                visibility: ServingEntityVisibility::Searchable,
                searchable_text: format!("{name} 3 BHK Whitefield"),
            },
        ];
        let facts = vec![
            fact(
                &property_id,
                "area",
                FactValue::Text("Whitefield".to_string()),
                learned_at,
            ),
            fact(
                &property_id,
                "carpet_area_sqft",
                FactValue::Numeric(size),
                learned_at,
            ),
            fact(&property_id, "bhk", FactValue::Numeric(3.0), learned_at),
            fact(
                &property_id,
                "price",
                FactValue::Numeric(15_000_000.0),
                learned_at,
            ),
            fact(
                &property_id,
                "builder_name",
                FactValue::Text("Fixture Builder".to_string()),
                learned_at,
            ),
        ];
        let search_metadata = facts
            .iter()
            .map(|fact| ServingSearchMetadataRecord {
                entity_id: fact.entity_id.clone(),
                fact_key: fact.fact_key.clone(),
                display_template: None,
                answers_preferences: Vec::new(),
                scoring_direction: None,
                scoring_weight: None,
                scoring_thresholds: Vec::new(),
            })
            .collect();
        CatalogRecords {
            entities,
            facts,
            search_metadata,
            edges: vec![ServingEdgeRecord {
                from_entity_id: property_id,
                edge_type: "in_society".to_string(),
                to_entity_id: society_id,
                confidence: 1.0,
                source_type: "Catalog".to_string(),
                derivation: None,
            }],
            rera_evidence: Vec::new(),
        }
    }

    fn fact(
        entity_id: &str,
        fact_key: &str,
        value: FactValue,
        learned_at: DateTime<Utc>,
    ) -> ServingFactRecord {
        ServingFactRecord {
            entity_id: entity_id.to_string(),
            fact_key: fact_key.to_string(),
            value_type: match value {
                FactValue::Numeric(_) => "numeric",
                _ => "text",
            }
            .to_string(),
            value_text: match &value {
                FactValue::Numeric(number) => Some(number.to_string()),
                FactValue::Text(text) => Some(text.clone()),
                _ => None,
            },
            value,
            confidence: 1.0,
            source_type: "Catalog".to_string(),
            source_url: None,
            model: None,
            skill_id: None,
            learned_at,
            observation: None,
        }
    }
}
