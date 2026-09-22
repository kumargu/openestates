/* Generated from Rust public DTOs. Run npm run contracts:generate. */

export type EvidenceId =
  | {
      id: string;
      kind: "observation";
    }
  | {
      id: string;
      kind: "derivation";
    }
  | {
      id: string;
      kind: "entity";
    }
  | {
      id: string;
      kind: "relationship";
    };

export interface PropertyEvidenceResponse {
  entity_refs: KgEntityRefs;
  property_id: string;
  sections: EvidenceSection[];
  serving_bundle_version?: string;
}
/**
 * Minimal entity identity bundle attached to property/search/detail responses.
 *
 * These fields are stable API identifiers, not display copy and not a complete
 * serving export. They exist so the UI can ask follow-up endpoints for richer
 * context when a user shows intent: opens a property, expands a card, compares
 * homes, clicks a source trail, or requests a nearby/risk/community breakdown.
 *
 * Current usage pattern:
 * 1. Render fast listing data from `PropertyCard` or `PropertyDetailResponse`.
 * 2. Use `source_entity_ids` as opaque provenance handles for the property,
 *    society, area, and builder.
 * 3. Use source/evidence read models when the UI needs a larger drill-down
 *    such as builder portfolio, nearby projects, or lineage.
 * 4. Build optional UI sections from facts with source/confidence metadata.
 * 5. Hide sections that have no backed facts instead of rendering empty cards.
 *
 * Important distinction: these are KG node IDs, not necessarily canonical RERA
 * IDs. Some societies have an alias node such as `society:prestige-park-grove`
 * while lake artifacts may also contain a RERA-rooted canonical ID. The UI
 * should not infer canonicalization from the string shape. It should treat the
 * IDs as opaque handles and follow the API.
 */
export interface KgEntityRefs {
  /**
   * Area/locality node for traffic, waterlogging, metro, schools, price trend,
   * and other externalities.
   */
  area_entity_id: string;
  /**
   * Builder node when the society has a known BuiltBy edge in the KG.
   */
  builder_entity_id?: string;
  /**
   * Listing-level node for facts specific to this flat/unit/listing.
   */
  property_entity_id: string;
  /**
   * Society/project node for RERA, reviews, nearby places, amenities, and
   * community evidence.
   */
  society_entity_id: string;
  /**
   * Existing graph nodes the UI can safely prefetch first.
   *
   * This list is backend-filtered to nodes present in the current KG, sorted,
   * and deduplicated. It may omit an otherwise valid field ID if that node has
   * not been materialized yet. UI code should treat it as a convenient fetch
   * plan, not as a complete semantic model.
   */
  source_entity_ids?: string[];
}
export interface EvidenceSection {
  community_pulse?: CommunityPulse;
  confidence_pct: number;
  constellation: string;
  entity_ids: string[];
  header_meta: string;
  items: SourceItem[];
  kind: string;
  media?: EvidenceMediaStrip[];
  missing: string[];
  presentation: EvidencePresentation;
  priority: number;
  relationship?: string;
  scope: string;
  source_types: string[];
  subtitle: string;
  summary: string;
  title: string;
}
export interface CommunityPulse {
  concerns: string[];
  paragraph: string;
  positives: string[];
  quotes: CommunityPulseQuote[];
  sentiment_band: string;
  source_label: string;
  source_urls: string[];
}
export interface CommunityPulseQuote {
  polarity: string;
  source_type: string;
  source_url?: string;
  text: string;
}
export interface SourceItem {
  attributions?: SourceAttribution[];
  entity_id: string;
  evidence: EvidenceRef[];
  key: string;
  label: string;
  learned_at: string;
  relationship?: string;
  scope: string;
  source_type: string;
  source_url?: string;
  value: string;
  values?: string[];
}
export interface SourceAttribution {
  evidence: EvidenceRef;
  learned_at: string;
  source_type: string;
  source_url?: string;
  value: string;
}
export interface EvidenceRef {
  evidence_id: EvidenceId;
  snapshot_identity: string;
  subject_entity_id: string;
}
export interface EvidenceMediaStrip {
  caption: string;
  capture_date_label: string;
  coverage_quality: string;
  frames: EvidenceMediaFrame[];
  kind: string;
  provider: string;
  title: string;
}
export interface EvidenceMediaFrame {
  capture_date: string;
  distance_from_gate_m: number;
  fov: number;
  heading: number;
  image_url: string;
  label: string;
  pitch: number;
  source_url: string;
}
export interface EvidencePresentation {
  density: string;
  max_preview_items: number;
  variant: string;
}
