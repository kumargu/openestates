# DAG config (`app/config/dag/`)

Control plane for the OpenEstates graph and pipeline. **Schemas only — no entity instances.**

| File | Primitive | Meaning |
|------|-----------|---------|
| `manifest.json` | meta | Package index, proof label thresholds, agent routing |
| `ontology.json` | **nodes + edges** | `entity_types` = node kinds; `relations` = allowed edge kinds |
| `concern_taxonomy.json` | **leaves** | 78 leaf definitions: `fact_key`, scopes, Reddit terms, buckets |
| `fact_registry.json` | **leaves** | Search/UI per leaf: `answers_preferences`, `scoring_hint`, `display_template` |
| `resolution_policies.json` | proof | Which source wins when facts conflict; confidence caps |
| `asset_registry.json` | pipeline | Asset DAG: what to crawl, enrich, materialize, in what order |
| `enrichment_targets.json` | enrichment | Re-run enrichment by leaf or UI surface; graph traverse rules |
| `ui_surfaces.json` | ui | Maps buyer surfaces (approach road, flooding…) → leaf_keys + components |
| `evidence_sections.json` | ui | Property detail evidence sections: metadata, presentation, fact keys, derived section modes |
| `search_intent.json` | search | Buyer archetypes; area alias migration target |
| `source_display_policy.json` | ui | Buyer-facing source labels and provenance visibility |
| `crawl_policies/*.json` | crawl | Per-source skip rules, cadence, isolated workers |
| `source_adapters/` | pipeline | (pending) One adapter contract per external source |

## Primitives quick reference

- **Node type** → `ontology.json` → `entity_types[]`
- **Edge type** → `ontology.json` → `relations[]`
- **Leaf definition** → `concern_taxonomy.json` → `buckets[].leaves[]`
- **Leaf semantics** → `fact_registry.json` → `facts[]`
- **Node/edge/leaf instances** → `data/lake/` Parquet (NOT here)

Regenerate merged files: `python3.10 pipeline/tools/build_dag_json.py`
