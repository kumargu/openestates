"""
Synthetic Market Generator

Produces buyers, sellers, and properties for the OpenEstates simulation.

Two-file output (enforced separation):
  synthetic_market.json       — visible attributes only (what the engine may read)
  synthetic_market_truth.json — hidden ground truth (evaluation layer only, engine must not load)

Usage:
    from simulation.market_generator import MarketGenerator
    gen = MarketGenerator(seed=42)
    result = gen.generate(num_buyers=150, num_sellers=200, num_properties=200)
    # result["market"]  → save as synthetic_market.json
    # result["truth"]   → save as synthetic_market_truth.json
"""

import datetime
import random
from dataclasses import asdict, dataclass, field
from typing import Dict, List


# --- Constants ---

AREAS = ["Whitefield", "Sarjapur", "HSR", "Bellandur", "Electronic City"]
FACINGS = ["North", "South", "East", "West", "North-East", "North-West"]
NEGOTIATION_STYLES = ["firm", "flexible", "very_flexible"]
URGENCY_LEVELS = ["low", "medium", "high"]
PRIVACY_LEVELS = ["low", "medium", "high"]

PRICE_MIN = 8_000_000   # 80 Lakhs
PRICE_MAX = 30_000_000  # 3 Crore
PRICE_STEP = 50_000     # Round to nearest 50K for realism


# --- Entity Schemas ---

@dataclass
class Property:
    id: str
    area: str
    price: int
    bhk: int
    floor: int
    facing: str
    metro_distance_minutes: int
    builder_quality_score: float    # 0.0 – 1.0
    society_quality_score: float    # 0.0 – 1.0
    litigation_risk: float          # 0.0 – 1.0, skewed low
    maintenance_cost: int           # monthly INR
    possession_status: str          # "ready" | "under_construction"
    document_completeness_score: float  # 0.0 – 1.0
    sunlight_score: float           # 0.0 – 1.0
    noise_score: float              # 0.0 – 1.0 (higher = noisier)
    hidden: Dict = field(default_factory=dict)


@dataclass
class Seller:
    id: str
    property_id: str
    urgency_level: str              # "low" | "medium" | "high"
    visit_tolerance: int            # max number of visits willing to allow
    negotiation_style: str          # "firm" | "flexible" | "very_flexible"
    price_flexibility: float        # fraction willing to drop, e.g. 0.07 = 7%
    possession_flexibility: bool    # willing to adjust handover timeline
    privacy_preference: str         # "low" | "medium" | "high"
    hidden: Dict = field(default_factory=dict)


@dataclass
class Buyer:
    id: str
    budget_min: int
    budget_max: int
    preferred_areas: List[str]
    preferred_bhk: List[int]
    timeline_months: int
    metro_preference_weight: float      # 0.0 – 1.0
    society_quality_weight: float       # 0.0 – 1.0
    document_safety_weight: float       # 0.0 – 1.0
    renovation_tolerance: float         # 0.0 – 1.0 (1 = fine with heavy reno)
    hidden: Dict = field(default_factory=dict)


# --- Helpers ---

def _split(entity_dict: dict):
    """Split an asdict() result into (visible_fields, hidden_fields)."""
    visible = {k: v for k, v in entity_dict.items() if k != "hidden"}
    hidden = entity_dict.get("hidden", {})
    return visible, hidden


# --- Generator ---

class MarketGenerator:
    """
    Generates a synthetic real estate market for OpenEstates simulation.
    Results are deterministic for a given seed.
    num_sellers must be <= num_properties (1 seller per property enforced).
    """

    def __init__(self, seed: int = 42):
        self.seed = seed
        self.rng = random.Random(seed)

    def _score(self) -> float:
        return round(self.rng.uniform(0.0, 1.0), 2)

    def _round_price(self, price: float) -> int:
        return int(round(price / PRICE_STEP) * PRICE_STEP)

    def _gen_property(self, idx: int) -> Property:
        area = self.rng.choice(AREAS)
        bhk = self.rng.choices([1, 2, 3, 4], weights=[5, 40, 40, 15], k=1)[0]
        price = self._round_price(self.rng.randint(PRICE_MIN, PRICE_MAX))
        litigation_risk = round(self.rng.betavariate(1, 8), 2)

        return Property(
            id=f"prop_{idx:04d}",
            area=area,
            price=price,
            bhk=bhk,
            floor=self.rng.randint(0, 20),
            facing=self.rng.choice(FACINGS),
            metro_distance_minutes=self.rng.randint(5, 45),
            builder_quality_score=self._score(),
            society_quality_score=self._score(),
            litigation_risk=litigation_risk,
            maintenance_cost=self.rng.randint(3_000, 15_000),
            possession_status=self.rng.choice(["ready", "under_construction"]),
            document_completeness_score=self._score(),
            sunlight_score=self._score(),
            noise_score=self._score(),
            hidden={
                "true_market_value": self._round_price(price * self.rng.uniform(0.85, 1.15)),
            },
        )

    def _gen_seller(self, idx: int, property_id: str, listed_price: int) -> Seller:
        urgency_level = self.rng.choice(URGENCY_LEVELS)
        price_flexibility = round(self.rng.uniform(0.0, 0.12), 3)

        urgency_ranges = {"low": (0.0, 0.35), "medium": (0.30, 0.65), "high": (0.60, 1.0)}
        lo, hi = urgency_ranges[urgency_level]
        true_urgency = round(self.rng.uniform(lo, hi), 3)
        hidden_buffer = self.rng.uniform(0.0, 0.03)

        return Seller(
            id=f"seller_{idx:04d}",
            property_id=property_id,
            urgency_level=urgency_level,
            visit_tolerance=self.rng.randint(2, 10),
            negotiation_style=self.rng.choice(NEGOTIATION_STYLES),
            price_flexibility=price_flexibility,
            possession_flexibility=self.rng.random() > 0.5,
            privacy_preference=self.rng.choice(PRIVACY_LEVELS),
            hidden={
                "true_price_floor": self._round_price(
                    listed_price * (1 - price_flexibility - hidden_buffer)
                ),
                "true_urgency": true_urgency,
                "friction_sensitivity": self._score(),
            },
        )

    def _gen_buyer(self, idx: int) -> Buyer:
        budget_max = self.rng.choices(
            [10_000_000, 15_000_000, 20_000_000, 25_000_000, 30_000_000],
            weights=[30, 30, 20, 15, 5],
            k=1,
        )[0]
        budget_min = self._round_price(budget_max * self.rng.uniform(0.5, 0.8))

        num_areas = self.rng.choices([1, 2, 3], weights=[40, 40, 20], k=1)[0]
        preferred_areas = self.rng.sample(AREAS, num_areas)

        # Fix: include 1BHK with realistic market weights
        primary_bhk = self.rng.choices([1, 2, 3, 4], weights=[10, 45, 35, 10], k=1)[0]
        # 40% chance to also accept adjacent higher BHK
        if self.rng.random() < 0.4 and primary_bhk < 4:
            preferred_bhk = sorted([primary_bhk, primary_bhk + 1])
        else:
            preferred_bhk = [primary_bhk]

        return Buyer(
            id=f"buyer_{idx:04d}",
            budget_min=budget_min,
            budget_max=budget_max,
            preferred_areas=preferred_areas,
            preferred_bhk=preferred_bhk,
            timeline_months=self.rng.randint(1, 24),
            metro_preference_weight=self._score(),
            society_quality_weight=self._score(),
            document_safety_weight=self._score(),
            renovation_tolerance=self._score(),
            hidden={
                "true_budget_limit": self._round_price(budget_max * self.rng.uniform(1.0, 1.15)),
                "hidden_area_flexibility": round(self.rng.uniform(0.0, 0.4), 3),
                "negotiation_patience": self._score(),
                "risk_tolerance": self._score(),
            },
        )

    def generate(
        self,
        num_buyers: int = 150,
        num_sellers: int = 200,
        num_properties: int = 200,
    ) -> dict:
        """
        Generate a full synthetic market.

        Returns:
            {
                "market": { meta, buyers, sellers, properties }  — visible only
                "truth":  { buyers, sellers, properties }        — hidden ground truth
            }

        The caller is responsible for saving these as separate files.
        The matching engine must only ever load market, never truth.
        """
        if num_sellers > num_properties:
            raise ValueError(
                f"num_sellers ({num_sellers}) cannot exceed num_properties ({num_properties}). "
                "Enforcing 1 seller per property."
            )

        properties = [self._gen_property(i) for i in range(num_properties)]

        # Shuffle so seller assignment is not ordered by property index
        assigned_props = properties[:num_sellers]
        self.rng.shuffle(assigned_props)

        sellers = [
            self._gen_seller(
                idx=i,
                property_id=assigned_props[i].id,
                listed_price=assigned_props[i].price,
            )
            for i in range(num_sellers)
        ]

        buyers = [self._gen_buyer(i) for i in range(num_buyers)]

        # Split visible vs hidden for each entity type
        visible_properties, truth_properties = [], {}
        for p in properties:
            d = asdict(p)
            vis, hid = _split(d)
            visible_properties.append(vis)
            truth_properties[vis["id"]] = hid

        visible_sellers, truth_sellers = [], {}
        for s in sellers:
            d = asdict(s)
            vis, hid = _split(d)
            visible_sellers.append(vis)
            truth_sellers[vis["id"]] = hid

        visible_buyers, truth_buyers = [], {}
        for b in buyers:
            d = asdict(b)
            vis, hid = _split(d)
            visible_buyers.append(vis)
            truth_buyers[vis["id"]] = hid

        meta = {
            "seed": self.seed,
            "generated_at": datetime.datetime.utcnow().isoformat() + "Z",
            "counts": {
                "buyers": len(buyers),
                "sellers": len(sellers),
                "properties": len(properties),
            },
        }

        return {
            "market": {
                "meta": meta,
                "buyers": visible_buyers,
                "sellers": visible_sellers,
                "properties": visible_properties,
            },
            "truth": {
                "meta": meta,
                "buyers": truth_buyers,
                "sellers": truth_sellers,
                "properties": truth_properties,
            },
        }
