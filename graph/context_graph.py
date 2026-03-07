"""
Context Graph — per-buyer signal store.

Stores structured preference signals with confidence, weight, provenance, and recency.
Supports deterministic apply/reinforce/weaken operations from SignalUpdate objects.
"""

import copy
import datetime
from typing import Dict, List, Optional

from agents.schemas import SignalUpdate


class Signal:
    """A single preference signal in the context graph."""

    __slots__ = ("value", "weight", "confidence", "updated_at", "sources", "repetition_count")

    def __init__(
        self,
        value: str,
        weight: float = 0.5,
        confidence: float = 0.5,
        updated_at: Optional[str] = None,
        sources: Optional[List[dict]] = None,
        repetition_count: int = 1,
    ):
        self.value = value
        self.weight = weight
        self.confidence = confidence
        self.updated_at = updated_at or datetime.datetime.utcnow().isoformat() + "Z"
        self.sources = sources or []
        self.repetition_count = repetition_count

    def to_dict(self) -> dict:
        return {
            "value": self.value,
            "weight": self.weight,
            "confidence": self.confidence,
            "updated_at": self.updated_at,
            "sources": self.sources,
            "repetition_count": self.repetition_count,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "Signal":
        return cls(
            value=d["value"],
            weight=d["weight"],
            confidence=d["confidence"],
            updated_at=d.get("updated_at"),
            sources=d.get("sources", []),
            repetition_count=d.get("repetition_count", 1),
        )


class ContextGraph:
    """
    Per-buyer context graph holding structured signals.

    Operations:
        snapshot()       — return a dict copy of current state
        apply_updates()  — apply a list of SignalUpdate objects, return a diff
    """

    def __init__(self, buyer_id: str, signals: Optional[Dict[str, Signal]] = None):
        self.buyer_id = buyer_id
        self.signals: Dict[str, Signal] = signals or {}

    def snapshot(self) -> dict:
        """Return a serializable copy of the current signal state."""
        return {
            "buyer_id": self.buyer_id,
            "signals": {k: s.to_dict() for k, s in self.signals.items()},
        }

    def apply_updates(self, updates: List[SignalUpdate]) -> List[dict]:
        """
        Apply a list of SignalUpdate objects. Returns a list of diffs.

        Each diff: {"key": ..., "action": ..., "before": ..., "after": ...}
        """
        now = datetime.datetime.utcnow().isoformat() + "Z"
        diffs = []

        for u in updates:
            before = self.signals[u.signal_key].to_dict() if u.signal_key in self.signals else None

            if u.action in ("add", "update"):
                self._apply_add_or_update(u, now)
            elif u.action == "weaken":
                self._apply_weaken(u, now)
            elif u.action == "remove":
                self._apply_remove(u)

            after = self.signals[u.signal_key].to_dict() if u.signal_key in self.signals else None

            diffs.append({
                "key": u.signal_key,
                "action": u.action,
                "before": before,
                "after": after,
            })

        return diffs

    def _apply_add_or_update(self, u: SignalUpdate, now: str):
        if u.signal_key in self.signals:
            sig = self.signals[u.signal_key]
            sig.value = u.signal_value
            # Reinforce: blend weight and confidence upward
            sig.weight = min(1.0, round((sig.weight + u.weight) / 2 + 0.05, 3))
            sig.confidence = min(1.0, round((sig.confidence + u.confidence) / 2 + 0.05, 3))
            sig.repetition_count += 1
            sig.updated_at = now
            sig.sources.append(u.provenance)
        else:
            self.signals[u.signal_key] = Signal(
                value=u.signal_value,
                weight=round(u.weight, 3),
                confidence=round(u.confidence, 3),
                updated_at=now,
                sources=[u.provenance],
                repetition_count=1,
            )

    def _apply_weaken(self, u: SignalUpdate, now: str):
        if u.signal_key in self.signals:
            sig = self.signals[u.signal_key]
            sig.weight = max(0.0, round(sig.weight - 0.15, 3))
            sig.confidence = max(0.0, round(sig.confidence - 0.1, 3))
            sig.updated_at = now
            sig.sources.append(u.provenance)

    def _apply_remove(self, u: SignalUpdate):
        self.signals.pop(u.signal_key, None)

    def to_dict(self) -> dict:
        return self.snapshot()

    @classmethod
    def from_dict(cls, data: dict) -> "ContextGraph":
        signals = {}
        for k, v in data.get("signals", {}).items():
            signals[k] = Signal.from_dict(v)
        return cls(buyer_id=data["buyer_id"], signals=signals)
