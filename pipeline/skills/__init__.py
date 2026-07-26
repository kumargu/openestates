"""
Skills Framework — structured extraction functions that feed the knowledge graph.

Each skill does one thing well: takes an input, produces typed SourcedFacts
with provenance, and writes them to the graph. Skills are composable, cacheable,
auditable, and LLM-agnostic.

Active skills:
  - search_reddit: Search Reddit directly for threads about a topic
  - fetch_rera: Fetch RERA registration facts from the government source
  - fetch_images: Fetch sourced property images
  - fetch_google_review_links: Fetch Google Maps review links and place metadata
  - market_pricing_facts: Normalize already-sourced marketplace prices
"""

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

__all__ = ["BaseSkill", "SkillResult", "SkillCost", "SourcedFact", "FactSource"]
