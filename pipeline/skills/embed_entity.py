"""
embed_entity — generate embeddings for entities using Google text-embedding-004.

Input: {
    "entity_id": "society:prestige-lakeside-habitat",
    "entity_type": "society",
    "name": "Prestige Lakeside Habitat",
    "summary": "Premium gated community in Whitefield",
    "signals": ["good-maintenance", "family-friendly"]
}
Output: SkillResult with embedding_computed fact and the actual vector in new_nodes.

Uses Google text-embedding-004 via REST (768-dim embeddings).
"""

import json
import logging
import os
from typing import List, Optional

from pipeline.skills.base import BaseSkill, SkillResult, SkillCost, SourcedFact, FactSource

logger = logging.getLogger(__name__)


def build_summary_text(input_data: dict) -> str:
    """Build a summary text string for embedding based on entity type."""
    entity_type = input_data.get("entity_type", "")
    name = input_data.get("name", "")

    if entity_type == "society":
        summary = input_data.get("summary", "")
        best_for = input_data.get("best_for", "")
        signals = input_data.get("signals", [])
        signals_str = ", ".join(signals) if signals else ""
        parts = [name]
        if summary:
            parts.append(summary)
        if best_for:
            parts.append(f"Best for: {best_for}")
        if signals_str:
            parts.append(f"Signals: {signals_str}")
        return ". ".join(parts)

    elif entity_type == "area":
        livability = input_data.get("livability_summary", "")
        metro = input_data.get("metro_access", "")
        vibe = input_data.get("vibe", "")
        parts = [name]
        if livability:
            parts.append(livability)
        if metro:
            parts.append(metro)
        if vibe:
            parts.append(vibe)
        return ". ".join(parts)

    elif entity_type == "property":
        title = input_data.get("title", name)
        desc = input_data.get("description_summary", "")
        area = input_data.get("area", "")
        tags = input_data.get("tags", [])
        tags_str = ", ".join(tags) if tags else ""
        parts = [title]
        if desc:
            parts.append(desc)
        if area:
            parts.append(area)
        if tags_str:
            parts.append(tags_str)
        return ". ".join(parts)

    else:
        # Fallback: concatenate name + summary
        summary = input_data.get("summary", "")
        return f"{name}. {summary}" if summary else name


def call_embedding_api(text: str) -> Optional[List[float]]:
    """Call Google text-embedding-004 API and return the embedding vector."""
    api_key = os.environ.get("GOOGLE_AI_API_KEY")
    if not api_key:
        logger.error("GOOGLE_AI_API_KEY not set")
        return None

    import urllib.request

    payload = json.dumps({
        "model": "models/gemini-embedding-001",
        "content": {
            "parts": [{"text": text}],
        },
    }).encode()

    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-001:embedContent?key={api_key}"

    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
    )

    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read().decode())
    except Exception as e:
        logger.error("Embedding API call failed: %s", e)
        return None

    try:
        values = data["embedding"]["values"]
        if not isinstance(values, list) or len(values) == 0:
            logger.error("Empty embedding returned")
            return None
        return values
    except KeyError:
        logger.error("Unexpected embedding response structure: %s", json.dumps(data)[:200])
        return None


class EmbedEntitySkill(BaseSkill):
    skill_id = "embed_entity"
    description = "Generate embeddings for entities using Google text-embedding-004"
    version = "1.0"
    output_keys = ["embedding_computed"]

    def execute(self, input_data: dict) -> SkillResult:
        entity_id = input_data.get("entity_id", "")
        entity_type = input_data.get("entity_type", "")
        name = input_data.get("name", "")

        if not entity_id or not name:
            logger.error("embed_entity requires entity_id and name")
            return SkillResult(confidence=0.0)

        # Step 1: Build summary text
        summary_text = build_summary_text(input_data)
        logger.info("Embedding text (%d chars): %s", len(summary_text), summary_text[:100])

        # Step 2: Call embedding API
        embedding = call_embedding_api(summary_text)
        if embedding is None:
            return SkillResult(
                confidence=0.0,
                cost=SkillCost(api_calls=1),
            )

        dims = len(embedding)

        # Step 3: Build result
        source = FactSource(
            source_type="Computed",
            skill_id=self.skill_id,
            triggered_by=input_data.get("triggered_by"),
        )

        # The fact records that embedding was computed (metadata)
        facts = [
            SourcedFact(
                key="embedding_computed",
                value={"type": "Bool", "data": True},
                confidence=1.0,
                source=source,
                display_template=f"Embedding computed ({dims} dims)",
            ),
        ]

        # The actual embedding vector goes into new_nodes for storage
        new_nodes = [
            {
                "node_id": entity_id,
                "summary_embedding": embedding,
            },
        ]

        return SkillResult(
            facts=facts,
            new_nodes=new_nodes,
            confidence=1.0,
            cost=SkillCost(
                llm_tokens=0,
                api_calls=1,
                estimated_usd=0.0001,  # Embedding API is very cheap
            ),
        )

    def estimated_cost(self) -> SkillCost:
        return SkillCost(llm_tokens=0, api_calls=1, estimated_usd=0.0001)


if __name__ == "__main__":
    """Quick test: python3 -m pipeline.skills.embed_entity"""
    import sys
    logging.basicConfig(level=logging.INFO)

    skill = EmbedEntitySkill()
    result = skill.run({
        "entity_id": "society:prestige-lakeside-habitat",
        "entity_type": "society",
        "name": "Prestige Lakeside Habitat",
        "summary": "Premium gated community in Whitefield",
        "signals": ["good-maintenance", "family-friendly"],
        "triggered_by": "manual_test",
    })

    print(f"\n=== Results: {len(result.facts)} facts, confidence={result.confidence:.2f} ===")
    print(f"Cost: {result.cost.api_calls} API calls, ${result.cost.estimated_usd:.4f}")
    print(f"Cached: {result.cached}")
    for fact in result.facts:
        print(f"  {fact.key}: {fact.value} (conf={fact.confidence}, src={fact.source.source_type})")
    if result.new_nodes:
        for node in result.new_nodes:
            emb = node.get("summary_embedding", [])
            print(f"  Embedding for {node['node_id']}: {len(emb)} dims, first 5: {emb[:5]}")
