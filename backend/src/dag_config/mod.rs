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
//! | `crawl_policies/*.json` | Crawl skip/cadence | Python collectors |
//!
//! **Instances** (society:*, road:*, fact values) live in `data/lake/` Parquet only.
//! See `app/config/coverage.json` for full audit.

mod evidence_sections;
mod fact_registry;
mod loader;
mod resolution;
mod search_guardrails;
mod search_intent;
mod ui_surfaces;

pub use evidence_sections::{
    evidence_sections_config, evidence_sections_path, load_evidence_sections,
    load_evidence_sections_from_path, ContextFactDefinition, EvidenceSectionDefinition,
    EvidenceSectionPresentation,
};
pub use fact_registry::{
    fact_registry_path, load_fact_registry, load_fact_registry_from_path, load_fact_registry_index,
    scoring_direction_from_hint, FactRegistryEntry, FactRegistryFile, FactRegistryIndex,
    FactRegistryRuntime, FactRegistryScoringHint,
};
pub use loader::{
    asset_registry_path, crawl_policy_path, dag_root, load_asset_registry, load_crawl_policy,
    load_json, load_manifest, set_project_dag_root, AssetRegistryFile, CrawlPolicyFile,
    DagConfigError, DagManifest,
};
pub use resolution::{
    better_source_type, better_source_type_for_fact, buyer_visible_fact, load_resolution_policies,
    source_tier_rank, ResolutionPoliciesFile,
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
pub use ui_surfaces::{
    load_ui_surfaces, load_ui_surfaces_from_path, ui_surfaces_config, ui_surfaces_path,
    UiSurfaceAnchorConfig, UiSurfaceConfig, UiSurfaceLayerRule, UiSurfaceSceneConfig,
    UiSurfacesFile,
};
