"""
Structured schemas for agent outputs.

SignalUpdate is the strict contract between OpenFang and our context graph.
Any extraction call — live or stubbed — must return List[SignalUpdate].
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional

VALID_ACTIONS = ("add", "update", "weaken", "remove")


@dataclass
class SignalUpdate:
    signal_key: str
    signal_value: str
    weight: float           # 0.0 – 1.0
    confidence: float       # 0.0 – 1.0
    action: str             # "add" | "update" | "weaken" | "remove"
    provenance: Dict = field(default_factory=lambda: {"turn_indices": []})

    def __post_init__(self):
        self.weight = max(0.0, min(1.0, self.weight))
        self.confidence = max(0.0, min(1.0, self.confidence))
        if self.action not in VALID_ACTIONS:
            raise ValueError(f"Invalid action '{self.action}', must be one of {VALID_ACTIONS}")

    def to_dict(self) -> dict:
        return {
            "signal_key": self.signal_key,
            "signal_value": self.signal_value,
            "weight": self.weight,
            "confidence": self.confidence,
            "action": self.action,
            "provenance": self.provenance,
        }


def parse_updates_response(raw_json: dict) -> List[SignalUpdate]:
    """
    Parse the strict JSON contract from OpenFang into SignalUpdate objects.

    Expected shape:
    {
      "updates": [
        {"signal_key": "...", "signal_value": "...", "weight": 0.8,
         "confidence": 0.7, "action": "add", "provenance": {"turn_indices": [0]}}
      ]
    }

    Raises ValueError if the shape doesn't match.
    """
    if not isinstance(raw_json, dict) or "updates" not in raw_json:
        raise ValueError("Response must be a JSON object with a single 'updates' key")

    updates = raw_json["updates"]
    if not isinstance(updates, list):
        raise ValueError("'updates' must be an array")

    result = []
    for item in updates:
        result.append(SignalUpdate(
            signal_key=str(item["signal_key"]),
            signal_value=str(item["signal_value"]),
            weight=float(item["weight"]),
            confidence=float(item["confidence"]),
            action=str(item["action"]),
            provenance=item.get("provenance", {"turn_indices": []}),
        ))
    return result
