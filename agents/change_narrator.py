"""
Change Narrator — generates human-readable summaries of context graph changes.

Two implementations:
  OpenFangNarrator  — calls OpenFang for natural language narrative
  StubNarrator      — deterministic template-based summary
"""

import json
from typing import List

from agents.openfang_client import OpenFangClient
from agents.schemas import SignalUpdate


NARRATOR_PROMPT = """\
You are a context change narrator for OpenEstates, a real estate matching system.

Given the before/after state of a buyer's context signals and the updates applied,
write a concise 1-2 sentence explanation of what changed and why.

Be specific. Reference actual signal names and values.
Example: "Strengthened metro preference to high after buyer mentioned it twice. Added budget flexibility — buyer willing to stretch if society quality is good."

RESPOND WITH ONLY the narrative text. No JSON, no markdown."""


class ChangeNarrator:
    """Base interface for change narration."""

    def narrate(
        self,
        before_context: dict,
        after_context: dict,
        updates: List[SignalUpdate],
    ) -> str:
        raise NotImplementedError


class OpenFangNarrator(ChangeNarrator):
    """Uses OpenFang LLM to generate a natural language narrative."""

    def __init__(self, client: OpenFangClient):
        self.client = client

    def narrate(
        self,
        before_context: dict,
        after_context: dict,
        updates: List[SignalUpdate],
    ) -> str:
        user_msg = (
            f"Before:\n{json.dumps(before_context.get('signals', {}), indent=2)}\n\n"
            f"After:\n{json.dumps(after_context.get('signals', {}), indent=2)}\n\n"
            f"Updates applied:\n{json.dumps([u.to_dict() for u in updates], indent=2)}"
        )
        return self.client.chat_completion(NARRATOR_PROMPT, user_msg)


class StubNarrator(ChangeNarrator):
    """Deterministic template-based narrator for offline use."""

    def narrate(
        self,
        before_context: dict,
        after_context: dict,
        updates: List[SignalUpdate],
    ) -> str:
        if not updates:
            return "No changes detected."

        parts = []
        for u in updates:
            if u.action == "add":
                parts.append(f"Added {u.signal_key} = {u.signal_value} (w:{u.weight:.1f})")
            elif u.action == "update":
                parts.append(f"Strengthened {u.signal_key} to {u.signal_value}")
            elif u.action == "weaken":
                parts.append(f"Weakened {u.signal_key}")
            elif u.action == "remove":
                parts.append(f"Removed {u.signal_key}")

        return ". ".join(parts) + "."
