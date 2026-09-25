/* Generated from Rust public DTOs. See the prototype README for regeneration. */

export type Availability = "active" | "disappeared" | "stale" | "unknown";
export type Relation = "confirmed" | "likely" | "comparable" | "registration";

export interface InventoryDetail {
  advertisements: Advertisement[];
  candidates: Advertisement[];
  comparables: Advertisement[];
  contract_version: number;
  fixture: boolean;
  registrations: Advertisement[];
  snapshot_id: string;
  summary: HomeSummary;
}
export interface Advertisement {
  /**
   * Explicit predecessor order, newest first. Never sorted by timestamps.
   */
  history: Observation[];
  identity: IdentitySignal[];
  observation: Observation;
}
export interface Observation {
  advertisement_id: string;
  amount_inr: number | null;
  area_basis: string | null;
  area_sqft: number | null;
  availability: Availability;
  bhk: number | null;
  current: boolean;
  first_seen: string | null;
  floor: number | null;
  home_id: string;
  id: string;
  last_seen: string | null;
  observed_on: string;
  predecessor_id: string | null;
  provider_id: string;
  relation: Relation;
  reliable: boolean;
  seller: string;
  source_label: string;
  source_url: string | null;
}
export interface IdentitySignal {
  disagrees: boolean;
  id: string;
  label: string;
  observation_id: string;
  value: string;
}
export interface HomeSummary {
  active_count: number;
  advertisement_count: number;
  ask: AskRange | null;
  conflicts: IdentitySignal[];
  home: Home;
  observed_on: string | null;
  price_change: PriceChange | null;
  uncertain_count: number;
}
export interface AskRange {
  max_inr: number;
  min_inr: number;
  observation_ids: string[];
}
export interface Home {
  area_basis: string | null;
  area_sqft: number | null;
  bhk: number;
  floor: number | null;
  id: string;
  image: string;
  location: string;
  scenario: string;
  title: string;
}
export interface PriceChange {
  difference_inr: number;
  observation_id: string;
  previous_observation_id: string;
}
