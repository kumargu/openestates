"""
reddit_resident_facts — derive concern_taxonomy signals from Reddit threads.

Emits canonical fact_keys only (no raw comment bodies in silver facts).
"""

from typing import Any, Dict, List, Optional

from pipeline.skills.base import BaseSkill, FactSource, SkillCost, SkillResult, SourcedFact
from pipeline.skills.reddit_theme_classifier import classify_corpus, load_reddit_theme_adapter


def threads_to_concern_facts(
    input_data: dict,
    threads: List[dict],
    adapter: Optional[dict] = None,
) -> SkillResult:
    adapter = adapter or load_reddit_theme_adapter()
    max_confidence = float(adapter.get("max_confidence") or 0.45)
    derived_value = str(adapter.get("derived_value") or "mentioned")
    source_type = str(adapter.get("source_type") or "RedditTheme")
    skill_id = str(adapter.get("skill_id") or "reddit_resident_facts")

    texts: List[str] = []
    source_url = None
    for thread in threads:
        title = str(thread.get("title") or "").strip()
        selftext = str(thread.get("selftext") or "").strip()
        if title:
            texts.append(title)
        if selftext:
            texts.append(selftext)
        if not source_url:
            source_url = thread.get("url")

    fact_keys = classify_corpus(texts, adapter=adapter)
    if not fact_keys:
        return SkillResult(
            facts=[],
            confidence=0.0,
            cost=SkillCost(api_calls=0),
        )

    facts: List[SourcedFact] = []
    for index, fact_key in enumerate(fact_keys):
        confidence = max(0.2, max_confidence - (index * 0.03))
        facts.append(
            SourcedFact(
                key=fact_key,
                value={"type": "Text", "data": derived_value},
                confidence=confidence,
                source=FactSource(
                    source_type=source_type,
                    url=source_url,
                    skill_id=skill_id,
                    triggered_by=input_data.get("triggered_by"),
                ),
                display_template="{value}",
            )
        )

    return SkillResult(
        facts=facts,
        confidence=max_confidence,
        cost=SkillCost(api_calls=1, estimated_usd=0.0003),
    )


class RedditResidentFactsSkill(BaseSkill):
    skill_id = "reddit_resident_facts"
    description = "Derive concern_taxonomy signals from Reddit threads (no raw text in facts)"
    version = "1.0"
    output_keys = []

    def execute(self, input_data: dict) -> SkillResult:
        threads = input_data.get("threads") or []
        if not isinstance(threads, list):
            threads = []
        return threads_to_concern_facts(input_data, threads)

    def estimated_cost(self) -> SkillCost:
        return SkillCost(api_calls=1, estimated_usd=0.0003)
