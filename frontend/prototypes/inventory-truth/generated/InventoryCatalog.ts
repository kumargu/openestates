/* Generated from Rust public DTOs. See the prototype README for regeneration. */

export interface InventoryCatalog {
  contract_version: number;
  fixture: boolean;
  homes: HomeSummary[];
  snapshot_id: string;
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
export interface IdentitySignal {
  disagrees: boolean;
  id: string;
  label: string;
  observation_id: string;
  value: string;
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
