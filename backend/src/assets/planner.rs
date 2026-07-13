use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::lake::LakeError;

use super::{
    AssetId, AssetMaterializationStore, AssetPartition, AssetRegistry, AssetStage, CostTier,
    MaterializationId, MaterializationRecord, PartitionResolutionError, RefreshCadence,
    RegistryError, TrustTier,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedAsset {
    pub asset_id: AssetId,
    pub reason: PlanReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetDagPlan {
    pub run_id: MaterializationId,
    pub partition: AssetPartition,
    pub planned_at: DateTime<Utc>,
    pub entries: Vec<AssetPlanEntry>,
}

impl AssetDagPlan {
    pub fn run_entries(&self) -> impl Iterator<Item = &AssetPlanEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.decision == PlanDecision::Run)
    }

    pub fn skipped_entries(&self) -> impl Iterator<Item = &AssetPlanEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.decision == PlanDecision::Skip)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPlanEntry {
    pub asset_id: AssetId,
    pub partition: AssetPartition,
    pub stage: AssetStage,
    pub dependencies: Vec<AssetId>,
    pub refresh: RefreshCadence,
    pub cost_tier: CostTier,
    pub trust_tier: TrustTier,
    pub decision: PlanDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<PlanReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_materialization_id: Option<MaterializationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_created_at: Option<DateTime<Utc>>,
    pub current_parent_materializations: Vec<MaterializationId>,
    pub freshness: AssetFreshness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    Run,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanReason {
    Missing,
    DependencyPending {
        asset_id: AssetId,
    },
    DependencyChanged {
        asset_id: AssetId,
    },
    Stale {
        cadence: RefreshCadence,
        age_seconds: i64,
        max_age_seconds: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetFreshness {
    pub cadence: RefreshCadence,
    pub reference_kind: FreshnessReferenceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_age_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<i64>,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessReferenceKind {
    MaterializedAt,
    SourceWatermark,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshnessPolicy {
    pub cadence: RefreshCadence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<i64>,
}

impl FreshnessPolicy {
    pub fn for_cadence(cadence: RefreshCadence) -> Self {
        let max_age = match cadence {
            RefreshCadence::Manual | RefreshCadence::OnChange => None,
            RefreshCadence::Daily => Some(Duration::days(1)),
            RefreshCadence::Weekly => Some(Duration::days(7)),
            RefreshCadence::Monthly => Some(Duration::days(31)),
            RefreshCadence::Quarterly => Some(Duration::days(92)),
        };
        Self {
            cadence,
            max_age_seconds: max_age.map(|duration| duration.num_seconds()),
        }
    }
}

#[derive(Debug)]
pub enum PlannerError {
    Registry(RegistryError),
    Partition(PartitionResolutionError),
    Lake(LakeError),
}

pub struct AssetPlanner {
    registry: AssetRegistry,
    materializations: AssetMaterializationStore,
}

impl AssetPlanner {
    pub fn new(registry: AssetRegistry, materializations: AssetMaterializationStore) -> Self {
        Self {
            registry,
            materializations,
        }
    }

    pub async fn plan_global(&self, now: DateTime<Utc>) -> Result<Vec<PlannedAsset>, PlannerError> {
        self.plan_partition(&AssetPartition::global(), now).await
    }

    pub async fn plan_partition(
        &self,
        partition: &AssetPartition,
        now: DateTime<Utc>,
    ) -> Result<Vec<PlannedAsset>, PlannerError> {
        let plan = self.plan_partition_details(partition, now).await?;
        Ok(plan
            .run_entries()
            .filter_map(|entry| {
                entry.reason.clone().map(|reason| PlannedAsset {
                    asset_id: entry.asset_id.clone(),
                    reason,
                })
            })
            .collect())
    }

    pub async fn plan_global_details(
        &self,
        now: DateTime<Utc>,
    ) -> Result<AssetDagPlan, PlannerError> {
        self.plan_partition_details(&AssetPartition::global(), now)
            .await
    }

    pub async fn plan_partition_details(
        &self,
        partition: &AssetPartition,
        now: DateTime<Utc>,
    ) -> Result<AssetDagPlan, PlannerError> {
        let ordered = self
            .registry
            .topological_order()
            .map_err(PlannerError::Registry)?;
        let mut records = HashMap::new();
        let mut planned_ids = HashSet::new();
        let mut entries = Vec::new();

        for asset_id in ordered {
            let definition = self.registry.get(&asset_id).expect("registry order asset");
            let asset_partition = definition
                .partition_policy
                .resolve(&asset_id, partition)
                .map_err(PlannerError::Partition)?;
            let current = self
                .materializations
                .current_record(&asset_id, &asset_partition)
                .await;
            let current = match current {
                Ok(record) => Some(record),
                Err(err) if err.is_not_found() => None,
                Err(err) => return Err(PlannerError::Lake(err)),
            };

            let reason = plan_reason(definition, current.as_ref(), &records, &planned_ids, now);
            if reason.is_some() {
                planned_ids.insert(asset_id.clone());
            }

            entries.push(plan_entry(
                definition,
                asset_partition,
                current.as_ref(),
                reason,
                now,
            ));
            records.insert(asset_id, current);
        }

        Ok(AssetDagPlan {
            run_id: MaterializationId::new(),
            partition: partition.clone(),
            planned_at: now,
            entries,
        })
    }
}

fn plan_entry(
    definition: &super::AssetDefinition,
    partition: AssetPartition,
    current: Option<&MaterializationRecord>,
    reason: Option<PlanReason>,
    now: DateTime<Utc>,
) -> AssetPlanEntry {
    let freshness = asset_freshness(definition.refresh, current, now);
    let decision = if reason.is_some() {
        PlanDecision::Run
    } else {
        PlanDecision::Skip
    };

    AssetPlanEntry {
        asset_id: definition.id.clone(),
        partition,
        stage: definition.stage,
        dependencies: definition.dependencies.clone(),
        refresh: definition.refresh,
        cost_tier: definition.cost_tier,
        trust_tier: definition.trust_tier,
        decision,
        reason,
        current_materialization_id: current.map(|record| record.materialization_id.clone()),
        current_version: current.map(|record| record.version.clone()),
        current_created_at: current.map(|record| record.created_at),
        current_parent_materializations: current
            .map(|record| record.parent_materializations.clone())
            .unwrap_or_default(),
        freshness,
    }
}

fn plan_reason(
    definition: &super::AssetDefinition,
    current: Option<&MaterializationRecord>,
    records: &HashMap<AssetId, Option<MaterializationRecord>>,
    planned_ids: &HashSet<AssetId>,
    now: DateTime<Utc>,
) -> Option<PlanReason> {
    let current = match current {
        Some(record) => record,
        None => return Some(PlanReason::Missing),
    };

    for dependency in &definition.dependencies {
        if planned_ids.contains(dependency) {
            return Some(PlanReason::DependencyPending {
                asset_id: dependency.clone(),
            });
        }

        let dependency_record = records.get(dependency).and_then(|record| record.as_ref());
        let dependency_record = match dependency_record {
            Some(record) => record,
            None => {
                return Some(PlanReason::DependencyPending {
                    asset_id: dependency.clone(),
                })
            }
        };
        if !current
            .parent_materializations
            .contains(&dependency_record.materialization_id)
        {
            return Some(PlanReason::DependencyChanged {
                asset_id: dependency.clone(),
            });
        }
    }

    let freshness = asset_freshness(definition.refresh, Some(current), now);
    if let (true, Some(age_seconds), Some(max_age_seconds)) = (
        freshness.is_stale,
        freshness.current_age_seconds,
        freshness.max_age_seconds,
    ) {
        return Some(PlanReason::Stale {
            cadence: definition.refresh,
            age_seconds,
            max_age_seconds,
        });
    }

    None
}

fn asset_freshness(
    cadence: RefreshCadence,
    current: Option<&MaterializationRecord>,
    now: DateTime<Utc>,
) -> AssetFreshness {
    let policy = FreshnessPolicy::for_cadence(cadence);
    let reference = current.map(freshness_reference);
    let current_age_seconds = reference.as_ref().map(|reference| {
        now.signed_duration_since(reference.time)
            .num_seconds()
            .max(0)
    });
    let is_stale = match (current_age_seconds, policy.max_age_seconds) {
        (Some(age), Some(max_age)) => age > max_age,
        _ => false,
    };
    let reference_kind = reference
        .as_ref()
        .map(|reference| reference.kind)
        .unwrap_or(FreshnessReferenceKind::Missing);

    AssetFreshness {
        cadence,
        reference_kind,
        reference_value: reference
            .as_ref()
            .and_then(|reference| reference.value.clone()),
        reference_time: reference.as_ref().map(|reference| reference.time),
        current_age_seconds,
        max_age_seconds: policy.max_age_seconds,
        is_stale,
    }
}

struct FreshnessReference {
    kind: FreshnessReferenceKind,
    value: Option<String>,
    time: DateTime<Utc>,
}

fn freshness_reference(record: &MaterializationRecord) -> FreshnessReference {
    let source_reference = record
        .source_watermarks
        .iter()
        .filter_map(|watermark| {
            parse_watermark_time(&watermark.high_watermark).map(|time| FreshnessReference {
                kind: FreshnessReferenceKind::SourceWatermark,
                value: Some(format!("{}:{}", watermark.source, watermark.high_watermark)),
                time,
            })
        })
        .min_by_key(|reference| reference.time);

    source_reference.unwrap_or(FreshnessReference {
        kind: FreshnessReferenceKind::MaterializedAt,
        value: None,
        time: record.created_at,
    })
}

fn parse_watermark_time(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(time) = DateTime::parse_from_rfc3339(value) {
        return Some(time.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date
            .and_hms_opt(0, 0, 0)
            .map(|time| Utc.from_utc_datetime(&time));
    }
    if value.len() == 7 {
        if let Ok(date) = NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d") {
            return date
                .and_hms_opt(0, 0, 0)
                .map(|time| Utc.from_utc_datetime(&time));
        }
    }
    None
}

impl std::fmt::Display for PlannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(err) => write!(f, "asset registry error: {err}"),
            Self::Partition(err) => write!(f, "asset partition resolution error: {err}"),
            Self::Lake(err) => write!(f, "asset lake error: {err}"),
        }
    }
}

impl std::error::Error for PlannerError {}
