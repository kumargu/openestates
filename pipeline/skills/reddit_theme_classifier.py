"""
Classify Reddit thread text into concern_taxonomy fact_keys.

Compliance: classifier reads raw text in-memory only; callers must not persist
matched snippets in silver/gold/serving fact values.
"""

import json
from pathlib import Path
from typing import Any, Dict, Iterable, List, Set

PROJECT_ROOT = Path(__file__).resolve().parents[2]
ADAPTER_PATH = PROJECT_ROOT / "app" / "config" / "dag" / "source_adapters" / "reddit_theme.json"


def load_reddit_theme_adapter() -> Dict[str, Any]:
    return json.loads(ADAPTER_PATH.read_text(encoding="utf-8"))


def normalize_text(value: str) -> str:
    return " ".join((value or "").lower().split())


def contains_term(haystack: str, term: str) -> bool:
    needle = normalize_text(term)
    if not needle:
        return False
    return needle in haystack


def classify_text(
    text: str,
    adapter: Dict[str, Any] = None,
) -> List[str]:
    """Return deduped concern_taxonomy fact_keys matched in text."""
    adapter = adapter or load_reddit_theme_adapter()
    haystack = normalize_text(text)
    if not haystack:
        return []

    matched: List[str] = []
    seen: Set[str] = set()
    for signal in adapter.get("signal_map", []):
        fact_key = str(signal.get("fact_key") or "").strip()
        if not fact_key or fact_key in seen:
            continue
        terms = signal.get("match_terms") or []
        if any(contains_term(haystack, str(term)) for term in terms):
            matched.append(fact_key)
            seen.add(fact_key)
    return matched


def classify_corpus(texts: Iterable[str], adapter: Dict[str, Any] = None) -> List[str]:
    keys: List[str] = []
    seen: Set[str] = set()
    for text in texts:
        for fact_key in classify_text(text, adapter=adapter):
            if fact_key not in seen:
                keys.append(fact_key)
                seen.add(fact_key)
    return keys
