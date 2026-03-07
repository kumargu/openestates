"""
Ground Truth Compatibility Model

Computes a hidden 'true' compatibility score for buyer-property pairs.
Used ONLY for evaluation and simulation — the matching engine must never import this.

The score is deterministic given the same inputs.

Score components and their evidence source:
  budget_fit        — initial design
  area_fit          — initial design
  bhk_fit           — initial design
  metro_fit         — initial design
  society_fit       — initial design
  environment_fit   — initial (noise) + Reddit: sewage plant, dump yard, highway posts
  doc_safety_fit    — initial design
  builder_risk_fit  — Reddit: Brigade cancellation, "3 instalments missed", RERA complaints
  timeline_fit      — initial design
  negotiation_fit   — initial design
  vastu_fit         — Reddit: "east-facing", vastu posts, directional preference recurrence
  possession_fit    — Reddit: "ready to move" vs "under construction" strong preference split

Weights live in data/truth_model_weights.json so they can evolve based on evidence.
Each weight carries a `rationale` explaining why it has its current value.
"""

import json
import os
from typing import Dict, List, Optional, Tuple

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WEIGHTS_PATH = os.path.join(PROJECT_ROOT, "data", "truth_model_weights.json")


# --- Score functions ---

def _clamp(value: float, lo: float = 0.0, hi: float = 1.0) -> float:
    return max(lo, min(hi, value))


def _budget_fit(property_price: int, buyer_true_budget: int, buyer_budget_min: int) -> float:
    if property_price > buyer_true_budget:
        overshoot = (property_price - buyer_true_budget) / buyer_true_budget
        return _clamp(1.0 - overshoot * 5.0)
    if property_price < buyer_budget_min:
        undershoot = (buyer_budget_min - property_price) / buyer_budget_min
        return _clamp(1.0 - undershoot * 2.0)
    return 1.0


def _area_fit(property_area: str, preferred_areas: List[str], area_flexibility: float) -> float:
    if property_area in preferred_areas:
        return 1.0
    return area_flexibility


def _bhk_fit(property_bhk: int, preferred_bhk: List[int]) -> float:
    if property_bhk in preferred_bhk:
        return 1.0
    min_distance = min(abs(property_bhk - b) for b in preferred_bhk)
    return 0.5 if min_distance == 1 else 0.0


def _metro_fit(metro_distance_minutes: int, weight: float) -> float:
    raw = _clamp(1.0 - (metro_distance_minutes - 5) / 40.0)
    return 1.0 - weight * (1.0 - raw)


def _society_fit(society_quality_score: float, weight: float) -> float:
    return 1.0 - weight * (1.0 - society_quality_score)


def _environment_fit(noise_score: float) -> float:
    """
    Broader than just noise — approximates sewage plant, dump yard, highway exposure.
    Reddit revealed environment sensitivity is a top decision driver.
    Uses noise_score as proxy (property generator uses it for overall environment quality).
    A more granular split (noise vs pollution vs proximity risks) would require richer data.
    """
    return 1.0 - noise_score


def _doc_safety_fit(doc_completeness: float, buyer_doc_weight: float) -> float:
    """Document completeness weighted by buyer concern for paperwork safety."""
    return 1.0 - buyer_doc_weight * (1.0 - doc_completeness)


def _builder_risk_fit(litigation_risk: float, buyer_risk_tolerance: float) -> float:
    """
    Reddit evidence: Brigade cancelled plots after 6 months, missed installation payments.
    Litigation risk (property attribute) captures builder/legal disputes.
    Buyer risk_tolerance (hidden) determines how much this matters.
    Low risk tolerance + high litigation risk = very bad fit.
    """
    # litigation_risk is 0.0 (safe) to 1.0 (risky)
    # buyer_risk_tolerance is 0.0 (cautious) to 1.0 (tolerant)
    # If buyer is risk-tolerant, litigation risk matters less
    sensitivity = 1.0 - buyer_risk_tolerance  # 0=tolerant, 1=cautious
    penalty = sensitivity * litigation_risk
    return _clamp(1.0 - penalty * 2.0)  # amplify: cautious buyer, risky property = bad


def _timeline_fit(seller_true_urgency: float, buyer_timeline_months: int) -> float:
    buyer_urgency = _clamp(1.0 - (buyer_timeline_months - 1) / 23.0)
    return 1.0 - abs(seller_true_urgency - buyer_urgency)


def _negotiation_fit(seller_style: str, buyer_patience: float) -> float:
    style_scores = {"very_flexible": 0.9, "flexible": 0.6, "firm": 0.2}
    seller_flex = style_scores.get(seller_style, 0.5)
    return _clamp(seller_flex * 0.5 + buyer_patience * 0.5)


def _vastu_fit(facing: str, buyer_risk_tolerance: float) -> float:
    """
    Reddit evidence: "east-facing" appeared repeatedly, dedicated vastu posts.
    Vastu preference is real but buyer-specific — not all buyers care.
    We approximate sensitivity from risk_tolerance: cautious buyers more culturally conservative.
    East and North facings are traditionally preferred.
    """
    preferred_facings = {"East": 1.0, "North": 0.9, "North-East": 0.85,
                         "North-West": 0.7, "West": 0.6, "South": 0.5}
    facing_score = preferred_facings.get(facing, 0.6)

    # vastu_sensitivity: cautious buyers (low risk tolerance) care more
    vastu_sensitivity = 1.0 - buyer_risk_tolerance  # 0=doesn't care, 1=cares a lot

    # If buyer doesn't care (sensitivity=0), facing is irrelevant → score 1.0
    # If buyer cares (sensitivity=1), facing score matters fully
    return 1.0 - vastu_sensitivity * (1.0 - facing_score)


def _possession_fit(
    possession_status: str,
    buyer_timeline_months: int,
    buyer_risk_tolerance: float,
) -> float:
    """
    Reddit evidence: clear preference split between RTM and under-construction.
    Short-timeline + risk-averse buyers strongly prefer ready-to-move.
    Long-timeline + risk-tolerant buyers are fine with under-construction.
    """
    is_ready = possession_status == "ready"

    if is_ready:
        return 1.0  # ready always works for anyone

    # Under construction: penalize based on timeline urgency and risk tolerance
    # Short timeline → can't wait for UC property
    timeline_urgency = _clamp(1.0 - (buyer_timeline_months - 1) / 23.0)  # 1=urgent, 0=patient
    # Low risk tolerance → uncomfortable with UC uncertainty
    risk_sensitivity = 1.0 - buyer_risk_tolerance

    # If buyer is urgent AND risk-averse, UC is a bad fit
    penalty = (timeline_urgency * 0.6 + risk_sensitivity * 0.4)
    return _clamp(1.0 - penalty * 0.5)  # softer penalty — UC is not a dealbreaker, just friction


# --- Weights config ---

DEFAULT_WEIGHTS = {
    "budget":        {"weight": 0.22, "rationale": "Hard constraint — core filter"},
    "area":          {"weight": 0.14, "rationale": "Strong stated preference"},
    "bhk":           {"weight": 0.13, "rationale": "Strong stated preference"},
    "metro":         {"weight": 0.07, "rationale": "Important but varies by buyer"},
    "society":       {"weight": 0.06, "rationale": "Quality of life signal"},
    "environment":   {"weight": 0.08, "rationale": "Raised from 0.05: Reddit sewage plant + dump yard posts show environment is a significant decision driver"},
    "doc_safety":    {"weight": 0.09, "rationale": "Legal safety — important for first-time buyers"},
    "builder_risk":  {"weight": 0.07, "rationale": "New: Reddit Brigade cancellation, missed instalments, RERA complaints"},
    "timeline":      {"weight": 0.07, "rationale": "Seller-buyer urgency alignment"},
    "negotiation":   {"weight": 0.05, "rationale": "Friction signal — less decisive than structural fit"},
    "vastu":         {"weight": 0.05, "rationale": "New: Reddit east-facing posts, vastu threads — cultural preference, buyer-weighted"},
    "possession":    {"weight": 0.07, "rationale": "New: Reddit RTM vs UC preference split — especially for short-timeline buyers"},
}


def _load_weights() -> Dict[str, Dict]:
    """Load weights from config file, falling back to defaults."""
    if os.path.exists(WEIGHTS_PATH):
        with open(WEIGHTS_PATH) as f:
            return json.load(f)
    return DEFAULT_WEIGHTS


def save_weights(weights: Dict[str, Dict]) -> None:
    """Persist weight config to file."""
    os.makedirs(os.path.dirname(WEIGHTS_PATH), exist_ok=True)
    with open(WEIGHTS_PATH, "w") as f:
        json.dump(weights, f, indent=2)


def initialize_weights_file() -> None:
    """Write defaults to file if not already there."""
    if not os.path.exists(WEIGHTS_PATH):
        save_weights(DEFAULT_WEIGHTS)


# --- Main scoring function ---

def compute_truth_compatibility(
    buyer_visible: dict,
    buyer_hidden: dict,
    seller_visible: dict,
    seller_hidden: dict,
    property_visible: dict,
    weights: Optional[Dict[str, Dict]] = None,
) -> Tuple[float, Dict[str, float]]:
    """
    Compute ground truth compatibility between a buyer and a property+seller pair.

    Returns:
        (score, breakdown) where score is [0.0, 1.0] and breakdown maps
        component names to individual scores.
    """
    w = weights or _load_weights()
    w_values = {k: v["weight"] for k, v in w.items()}

    breakdown = {
        "budget": _budget_fit(
            property_visible["price"],
            buyer_hidden["true_budget_limit"],
            buyer_visible["budget_min"],
        ),
        "area": _area_fit(
            property_visible["area"],
            buyer_visible["preferred_areas"],
            buyer_hidden["hidden_area_flexibility"],
        ),
        "bhk": _bhk_fit(
            property_visible["bhk"],
            buyer_visible["preferred_bhk"],
        ),
        "metro": _metro_fit(
            property_visible["metro_distance_minutes"],
            buyer_visible["metro_preference_weight"],
        ),
        "society": _society_fit(
            property_visible["society_quality_score"],
            buyer_visible["society_quality_weight"],
        ),
        "environment": _environment_fit(
            property_visible["noise_score"],
        ),
        "doc_safety": _doc_safety_fit(
            property_visible["document_completeness_score"],
            buyer_visible["document_safety_weight"],
        ),
        "builder_risk": _builder_risk_fit(
            property_visible["litigation_risk"],
            buyer_hidden["risk_tolerance"],
        ),
        "timeline": _timeline_fit(
            seller_hidden["true_urgency"],
            buyer_visible["timeline_months"],
        ),
        "negotiation": _negotiation_fit(
            seller_visible["negotiation_style"],
            buyer_hidden["negotiation_patience"],
        ),
        "vastu": _vastu_fit(
            property_visible["facing"],
            buyer_hidden["risk_tolerance"],
        ),
        "possession": _possession_fit(
            property_visible["possession_status"],
            buyer_visible["timeline_months"],
            buyer_hidden["risk_tolerance"],
        ),
    }

    # Only score components that have weights defined
    scored_keys = [k for k in breakdown if k in w_values]
    total = sum(w_values[k] * breakdown[k] for k in scored_keys)
    weight_sum = sum(w_values[k] for k in scored_keys)
    score = round(total / weight_sum, 4) if weight_sum > 0 else 0.0

    breakdown = {k: round(v, 3) for k, v in breakdown.items()}
    return score, breakdown


def rank_properties_for_buyer(
    buyer_id: str,
    market: dict,
    truth: dict,
    top_k: int = 5,
) -> List[dict]:
    """
    Rank all properties for a given buyer by truth compatibility score.
    """
    buyer_visible = None
    for b in market["buyers"]:
        if b["id"] == buyer_id:
            buyer_visible = b
            break
    if buyer_visible is None:
        raise ValueError(f"Buyer {buyer_id} not found in market data")

    buyer_hidden = truth["buyers"].get(buyer_id, {})
    if not buyer_hidden:
        raise ValueError(f"Buyer {buyer_id} not found in truth data")

    seller_by_property = {s["property_id"]: s for s in market["sellers"]}

    results = []
    for prop in market["properties"]:
        seller_visible = seller_by_property.get(prop["id"])
        if not seller_visible:
            continue

        seller_hidden = truth["sellers"].get(seller_visible["id"], {})
        if not seller_hidden:
            continue

        score, breakdown = compute_truth_compatibility(
            buyer_visible=buyer_visible,
            buyer_hidden=buyer_hidden,
            seller_visible=seller_visible,
            seller_hidden=seller_hidden,
            property_visible=prop,
        )
        results.append({
            "property_id": prop["id"],
            "seller_id": seller_visible["id"],
            "score": score,
            "breakdown": breakdown,
        })

    results.sort(key=lambda r: r["score"], reverse=True)
    return results[:top_k]


def show_weights() -> List[str]:
    """Return formatted weight table for display."""
    w = _load_weights()
    lines = ["Truth model weights (evidence-based):"]
    total = sum(v["weight"] for v in w.values())
    for component, cfg in w.items():
        pct = cfg["weight"] / total * 100
        bar = "█" * int(pct / 2)
        lines.append(f"  {component:14s} {cfg['weight']:.2f} ({pct:4.1f}%)  {bar}")
        lines.append(f"               {cfg['rationale']}")
    return lines
