"""
Skills Framework — structured extraction functions that feed the knowledge graph.

Each skill does one thing well: takes an input, produces typed SourcedFacts
with provenance, and writes them to the graph. Skills are composable, cacheable,
auditable, and LLM-agnostic.

Available skills:
  - search_reddit: Search Reddit for threads about a topic
  - learn_society: Enrich a society node from Reddit + Claude synthesis
  - learn_area: Enrich an area node via Gemini grounded search
  - verify_rera: Verify RERA registration (government source, confidence 1.0)
  - fetch_google_reviews: Extract Google review intelligence via Gemini
  - embed_entity: Generate text-embedding-004 embeddings for entities
"""

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

__all__ = ["BaseSkill", "SkillResult", "SkillCost", "SourcedFact", "FactSource"]
