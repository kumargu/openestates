"""
Signal Extractor — converts conversation turns into structured SignalUpdate objects.

Two implementations:
  OpenFangExtractor  — calls OpenFang /v1/chat/completions
  StubExtractor      — deterministic keyword matching (offline fallback)
"""

import json
from typing import Dict, List

from agents.openfang_client import OpenFangClient
from agents.schemas import SignalUpdate, parse_updates_response


SYSTEM_PROMPT = """\
You are a real estate signal extractor for the OpenEstates matching system.

Given a buyer's current profile and their recent conversation messages,
extract structured preference signals that affect property matching quality.

RESPOND WITH ONLY a JSON object in this EXACT format — no markdown, no prose:
{
  "updates": [
    {
      "signal_key": "snake_case_signal_name",
      "signal_value": "the preference value",
      "weight": 0.0,
      "confidence": 0.0,
      "action": "add",
      "provenance": {"turn_indices": [0]}
    }
  ]
}

Signal keys should be descriptive like: metro_preference, budget_flexibility,
document_safety, society_quality_preference, timeline_urgency,
renovation_tolerance, noise_tolerance, facing_preference, floor_preference,
possession_timeline, commute_tolerance, financing_status.

Actions:
- "add": new signal not in current context
- "update": strengthen or change an existing signal
- "weaken": reduce confidence in an existing signal
- "remove": signal is contradicted or no longer relevant

weight and confidence are floats in [0.0, 1.0].
turn_indices refer to the 0-based index in the conversation list.
Only output the JSON object. Nothing else."""


def _build_user_message(buyer_profile: dict, context_snapshot: dict, turns: List[str]) -> str:
    turns_block = "\n".join(f"[{i}] \"{t}\"" for i, t in enumerate(turns))
    return (
        f"Buyer profile:\n{json.dumps(buyer_profile, indent=2)}\n\n"
        f"Current context signals:\n{json.dumps(context_snapshot.get('signals', {}), indent=2)}\n\n"
        f"Recent conversation:\n{turns_block}"
    )


class SignalExtractor:
    """Base interface for signal extraction."""

    def extract(
        self,
        buyer_id: str,
        buyer_profile: dict,
        context_snapshot: dict,
        conversation_turns: List[str],
    ) -> List[SignalUpdate]:
        raise NotImplementedError


class OpenFangExtractor(SignalExtractor):
    """Calls OpenFang to extract signals via LLM."""

    def __init__(self, client: OpenFangClient):
        self.client = client

    def extract(
        self,
        buyer_id: str,
        buyer_profile: dict,
        context_snapshot: dict,
        conversation_turns: List[str],
    ) -> List[SignalUpdate]:
        user_msg = _build_user_message(buyer_profile, context_snapshot, conversation_turns)
        raw_content = self.client.chat_completion(SYSTEM_PROMPT, user_msg)

        try:
            raw_json = json.loads(raw_content)
        except json.JSONDecodeError as e:
            raise ValueError(f"OpenFang returned invalid JSON: {e}")

        return parse_updates_response(raw_json)


# --- Stub fallback (deterministic, keyword-based) ---

KEYWORD_MAP = {
    "metro":       ("metro_preference",           "high"),
    "budget":      ("budget_flexibility",          "flexible"),
    "stretch":     ("budget_flexibility",          "flexible"),
    "document":    ("document_safety",             "high"),
    "legal":       ("document_safety",             "high"),
    "society":     ("society_quality_preference",  "high"),
    "timeline":    ("timeline_urgency",            "urgent"),
    "months":      ("timeline_urgency",            "urgent"),
    "loan":        ("financing_status",            "in_progress"),
    "possession":  ("possession_timeline",         "important"),
    "renovation":  ("renovation_tolerance",        "low"),
    "noise":       ("noise_tolerance",             "low"),
    "sunlight":    ("sunlight_preference",         "high"),
    "floor":       ("floor_preference",            "mentioned"),
    "facing":      ("facing_preference",           "mentioned"),
    "commute":     ("commute_tolerance",           "important"),
}


class StubExtractor(SignalExtractor):
    """
    Deterministic keyword-based extractor for offline/testing use.
    Scans conversation turns for known keywords and produces signals.
    """

    def extract(
        self,
        buyer_id: str,
        buyer_profile: dict,
        context_snapshot: dict,
        conversation_turns: List[str],
    ) -> List[SignalUpdate]:
        found: Dict[str, List[int]] = {}

        for i, turn in enumerate(conversation_turns):
            lower = turn.lower()
            for keyword, (signal_key, _) in KEYWORD_MAP.items():
                if keyword in lower:
                    found.setdefault(signal_key, []).append(i)

        existing_keys = set(context_snapshot.get("signals", {}).keys())
        updates = []
        for signal_key, turn_indices in found.items():
            _, signal_value = next(
                (v for k, v in KEYWORD_MAP.items() if v[0] == signal_key),
                (signal_key, "detected"),
            )
            action = "update" if signal_key in existing_keys else "add"
            updates.append(SignalUpdate(
                signal_key=signal_key,
                signal_value=signal_value,
                weight=0.6,
                confidence=0.5,
                action=action,
                provenance={"turn_indices": turn_indices},
            ))

        return updates
