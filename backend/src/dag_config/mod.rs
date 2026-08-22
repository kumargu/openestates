//! DAG control-plane config loaders (`app/config/dag/`).
//!
//! Git is the source of truth for graph **schemas** (not instances).
//!
//! | File | Primitive | Loaded by |
//! |------|-----------|-----------|
//! | `ontology.json` | Node + edge **types** | Future: KG validator |
//! | `concern_taxonomy.json` | Leaf **definitions** | Enrichment skills |
//! | `fact_registry.json` | Leaf **search semantics** | Materializer → search_metadata |
//! | `resolution_policies.json` | Source conflict rules | Fact resolver |
//! | `asset_registry.json` | Pipeline asset DAG | `openestates_registry()` |
//! | `enrichment_targets.json` | Re-enrichment plans | `openestates-enrich` (pending) |
//! | `ui_surfaces.json` | UI surface → leaves | Frontend/API mappers |
//! | `evidence_sections.json` | Property evidence section metadata | Property detail API |
//! | `search_intent.json` | Buyer archetypes | Search intent (migration pending) |
//! | `serving_eligibility.json` | Clean-bundle admission policy | Serving bundle builder + release validator |
//! | `crawl_policies/*.json` | Crawl skip/cadence | Python collectors |
//!
//! **Instances** (society:*, road:*, fact values) live in `data/lake/` Parquet only.
//! See `app/config/coverage.json` for full audit.

mod community_themes;
mod concern_taxonomy;
mod evidence_sections;
mod fact_registry;
mod loader;
mod nearby_place_categories;
mod rera_decision_labels;
mod rera_report_surface;
mod resolution;
mod search_guardrails;
mod search_intent;
mod serving_eligibility;
mod ui_surfaces;

pub use community_themes::{
    community_themes_config, community_themes_path, load_community_themes,
    load_community_themes_from_path, CommunityEmbeddingExpansion,
    CommunityEmbeddingExpansionConfig, CommunityThemeDefinition, CommunityThemesFile,
};
pub use concern_taxonomy::{
    concern_taxonomy_path, load_concern_taxonomy, load_concern_taxonomy_from_path,
    ConcernTaxonomyBucket, ConcernTaxonomyFile, ConcernTaxonomyLeaf,
};
pub use evidence_sections::{
    evidence_sections_config, evidence_sections_path, load_evidence_sections,
    load_evidence_sections_from_path, ContextFactDefinition, EvidenceSectionDefinition,
    EvidenceSectionPresentation,
};
pub use fact_registry::{
    fact_registry_index_config, fact_registry_path, load_fact_registry,
    load_fact_registry_from_path, load_fact_registry_index, scoring_direction_from_hint,
    FactRegistryEntry, FactRegistryEvidenceDimension, FactRegistryFile, FactRegistryIndex,
    FactRegistryQueryUnit, FactRegistryRuntime, FactRegistryScoringHint,
    FactRegistrySearchDimension,
};
pub use loader::{
    asset_registry_path, crawl_policy_path, dag_root, load_asset_registry, load_crawl_policy,
    load_json, load_manifest, set_project_dag_root, AssetRegistryFile, CrawlPolicyFile,
    DagConfigError, DagManifest,
};
pub use nearby_place_categories::{
    load_nearby_place_categories, load_nearby_place_categories_from_path,
    nearby_place_categories_config, nearby_place_categories_path,
    nearby_place_category_for_fact_key, nearby_place_fact_key_matches_category,
    requested_nearby_place_categories, DerivedDistanceRisk, NearbyPlaceCategoriesFile,
    NearbyPlaceCategory,
};
pub use rera_decision_labels::{
    load_rera_decision_labels, load_rera_decision_labels_from_path, rera_decision_labels_config,
    rera_decision_labels_path, ReraDecisionLabelCondition, ReraDecisionLabelDefinition,
    ReraDecisionLabelGroupDefinition, ReraDecisionLabelSource, ReraDecisionLabelSummaryConfig,
    ReraDecisionLabelsFile,
};
pub use rera_report_surface::{
    load_rera_report_surface, load_rera_report_surface_from_path, rera_report_surface_config,
    rera_report_surface_path, ReraReportCandidateRules, ReraReportDisplayRule,
    ReraReportNotebookLabelRule, ReraReportNumericUnitRule, ReraReportSectionRule,
    ReraReportSelectorRule, ReraReportSurfaceFile, ReraReportToneCondition, ReraReportToneRule,
    ReraReportValueRules,
};
pub use resolution::{
    better_source_type, better_source_type_for_fact, buyer_visible_fact, coordinate_source_allowed,
    load_resolution_policies, normalize_source_type, resolve_coordinate_pair,
    source_allowed_for_fact, source_tier_rank, valid_coordinate_pair, CoordinateEntityScope,
    CoordinatePairCandidate, CoordinateSourcePolicy, ResolutionPoliciesFile,
    ResolvedCoordinatePair,
};
pub use search_guardrails::{
    load_search_guardrails, load_search_guardrails_from_path, search_guardrail_config,
    search_guardrails_path, AssistantDirectedQuestionConfig, HomeIntentDetectionConfig,
    PhraseGuardrailConfig, SearchGuardrailFile, SearchGuardrailGuidanceConfig,
    SearchGuidanceTemplate, StructuredSignalScores, TooShortGuardrailConfig, WeightedTermGroup,
};
pub use search_intent::{
    area_alias_entries, load_search_intent, load_search_intent_from_path, search_intent_path,
    search_parser_config, search_resolution_config, AreaAliasEntry, BhkParserConfig, NumberWord,
    RelationAliasConfig, RelationParserConfig, SearchIntentFile, SearchParserConfig,
    SearchPlaceFamilyAlias, SearchResolutionConfig, UnitAliasConfig, UnitValueParserConfig,
};
pub use serving_eligibility::{
    load_search_experiment_eligibility, load_serving_eligibility,
    load_serving_eligibility_from_path, search_experiment_eligibility_path,
    serving_eligibility_path, EligibilityValuePredicate, ProjectedPropertyRequirement,
    ServingAdmissionProfile, ServingEligibilityFile, SocietyEvidenceRequirement,
};
pub use ui_surfaces::{
    load_ui_surfaces, load_ui_surfaces_from_path, ui_surfaces_config, ui_surfaces_path,
    UiSurfaceAnchorConfig, UiSurfaceConfig, UiSurfaceLayerRule, UiSurfaceSceneConfig,
    UiSurfacesFile,
};
