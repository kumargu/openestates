#!/usr/bin/env python3
"""Build merged DAG config JSON from concern_taxonomy + legacy registries."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Dict, List, Optional

ROOT = Path(__file__).resolve().parents[2]
DAG = ROOT / "app" / "config" / "dag"


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2)
        handle.write("\n")


def legacy_fact_key_map(fact_schema: Dict[str, Any]) -> Dict[str, str]:
    mapping: Dict[str, str] = {}
    for layer in fact_schema.get("theme_layers", []):
        for key in layer.get("fact_keys", []):
            mapping[key] = key
    for section in ("text_evidence", "numeric_evidence"):
        for entry in fact_schema.get(section, []):
            for key in entry.get("fact_keys", []):
                mapping[key] = key
    return mapping


def schema_hints_for_fact_key(
    fact_key: str, fact_schema: Dict[str, Any], legacy_map: Dict[str, str]
) -> Dict[str, Any]:
    candidates = {fact_key}
    short = fact_key.split(".")[-1]
    candidates.add(short)
    for legacy, canonical in legacy_map.items():
        if canonical == fact_key or legacy == short:
            candidates.add(legacy)

    for entry in fact_schema.get("text_evidence", []):
        keys = set(entry.get("fact_keys", []))
        if keys & candidates:
            polarity = "concern" if any(
                term in entry.get("negative_terms", [])
                for term in ("waterlogging", "traffic", "noise", "delay", "litigation")
            ) else "positive"
            return {
                "answers_preferences": list(
                    dict.fromkeys(
                        entry.get("aliases", [])
                        + [entry.get("label", "")]
                        + entry.get("positive_terms", [])[:4]
                    )
                ),
                "scoring_hint": {
                    "direction": "text_match",
                    "weight": entry.get("score_delta", 1.0),
                    "polarity": polarity,
                },
                "display_template": entry.get("display_label", "{value}"),
            }

    for entry in fact_schema.get("numeric_evidence", []):
        keys = set(entry.get("fact_keys", []))
        if keys & candidates:
            direction = entry.get("direction", "LowerIsBetter")
            return {
                "answers_preferences": entry.get("aliases", []) + [entry.get("label", "")],
                "scoring_hint": {
                    "direction": "numeric",
                    "numeric_direction": direction,
                    "weight": entry.get("score_delta", 1.0),
                    "thresholds": entry.get("thresholds", []),
                },
                "display_template": entry.get("display_label", "{value}"),
            }

    for pos in fact_schema.get("positive_preference_patterns", []):
        if set(pos.get("expanded_keys", [])) & candidates:
            return {
                "answers_preferences": pos.get("patterns", []) + [pos.get("label", "")],
                "scoring_hint": {"direction": "text_match", "weight": pos.get("weight", 1.0)},
                "display_template": pos.get("label", "{value}"),
            }

    for neg in fact_schema.get("negative_preference_patterns", []):
        if set(neg.get("expanded_keys", [])) & candidates:
            return {
                "answers_preferences": neg.get("patterns", []) + [neg.get("label", "")],
                "scoring_hint": {
                    "direction": "text_match",
                    "weight": neg.get("weight", 1.0),
                    "polarity": "concern",
                },
                "display_template": neg.get("label", "{value}"),
            }

    return {}


def build_fact_registry() -> Dict[str, Any]:
    taxonomy = load_json(DAG / "concern_taxonomy.json")
    existing = load_json(DAG / "fact_registry.json")
    legacy_map = taxonomy.get("legacy_key_map", {})
    legacy_schema = legacy_fact_key_map(existing)

    facts: List[Dict[str, Any]] = []
    for bucket in taxonomy.get("buckets", []):
        for leaf in bucket.get("leaves", []):
            fact_key = leaf["fact_key"]
            hints = schema_hints_for_fact_key(fact_key, existing, legacy_schema)
            preferences = list(
                dict.fromkeys(
                    leaf.get("preferences", [])
                    + hints.get("answers_preferences", [])
                )
            )
            entry: Dict[str, Any] = {
                "fact_key": fact_key,
                "bucket": bucket["id"],
                "label": leaf.get("label", fact_key),
                "lens": leaf.get("lens", "operating"),
                "polarity": leaf.get("polarity", "neutral"),
                "scopes": leaf.get("scopes", ["society"]),
                "enrichment_terms": leaf.get("terms", []),
                "answers_preferences": [p for p in preferences if p],
                "never_default": taxonomy.get("defaults", {}).get("never_default", True),
                "source_types": taxonomy.get("defaults", {}).get("source_types", []),
            }
            if leaf.get("issue2_key"):
                entry["issue2_key"] = leaf["issue2_key"]
            if hints.get("display_template"):
                entry["display_template"] = hints["display_template"]
            elif leaf.get("label"):
                entry["display_template"] = f"{leaf['label']}: {{value}}"
            if hints.get("scoring_hint"):
                entry["scoring_hint"] = hints["scoring_hint"]
            else:
                weight = 1.2 if leaf.get("polarity") == "concern" else 0.9
                entry["scoring_hint"] = {"direction": "text_match", "weight": weight}
            facts.append(entry)

    return {
        "version": 1,
        "_comment": "LEAF SEARCH SEMANTICS — answers_preferences, scoring_hint, display_template per fact_key. Merged from concern_taxonomy + fact_schema_registry.",
        "description": "Canonical leaf registry: search semantics + enrichment metadata per fact_key.",
        "merged_from": [
            "app/config/dag/concern_taxonomy.json",
            "app/config/dag/fact_registry.json",
        ],
        "defaults": taxonomy.get("defaults", {}),
        "legacy_key_map": legacy_map,
        "search_dimensions": existing.get("search_dimensions", []),
        "preference_patterns": existing.get("preference_patterns", {}),
        "numeric_constraints": existing.get("numeric_constraints", []),
        "text_evidence": existing.get("text_evidence", []),
        "numeric_evidence": existing.get("numeric_evidence", []),
        "facts": facts,
        "fact_count": len(facts),
    }


def leaf_keys_for_surface(surface_id: str, facts: List[Dict[str, Any]]) -> List[str]:
    patterns = {
        "approach_road": ["approach_road", "road_width", "access_road", "road_segment"],
        "water_utilities": ["water", "tanker", "bwssb", "borewell", "cauvery"],
        "flooding_drainage": ["waterlogging", "flooding", "drain", "nala", "rajakaluve", "seepage"],
        "litigation_legal": ["litigation", "rera", "legal", "complaint", "oc_"],
        "home_state": ["home_state", "possession", "delivered", "construction", "oc_"],
        "builder_trust": ["builder", "delay", "revocation"],
        "metro_commute": ["metro", "commute", "traffic"],
        "livability_positive": [],
    }
    keys = patterns.get(surface_id, [surface_id.replace("_", ".")])
    if surface_id == "livability_positive":
        return [
            f["fact_key"]
            for f in facts
            if f.get("polarity") == "positive" and f.get("lens") in ("positive", "lifecycle")
        ]
    matched = []
    for fact in facts:
        haystack = " ".join(
            [fact["fact_key"], fact.get("label", ""), fact.get("bucket", "")]
            + fact.get("enrichment_terms", [])
        ).lower()
        if any(p in haystack for p in keys):
            matched.append(fact["fact_key"])
    return sorted(dict.fromkeys(matched))


def build_enrichment_targets(facts: List[Dict[str, Any]]) -> Dict[str, Any]:
    surfaces = [
        "approach_road",
        "water_utilities",
        "flooding_drainage",
        "litigation_legal",
        "home_state",
        "builder_trust",
        "metro_commute",
        "livability_positive",
    ]
    targets: List[Dict[str, Any]] = []

    for surface_id in surfaces:
        leaf_keys = leaf_keys_for_surface(surface_id, facts)
        if not leaf_keys:
            continue
        scopes = sorted(
            {
                scope
                for key in leaf_keys
                for scope in next(f["scopes"] for f in facts if f["fact_key"] == key)
            }
        )
        traverse = []
        if "road_segment" in scopes:
            traverse.append(
                {
                    "from": "society",
                    "edge": "served_by_road",
                    "to": "road_segment",
                    "when": "leaf_scope_includes_road_segment",
                    "project_facts_to": ["society", "property"],
                }
            )
        if "area" in scopes:
            traverse.append(
                {
                    "from": "society",
                    "edge": "in_area",
                    "to": "area",
                    "when": "leaf_scope_includes_area",
                    "project_facts_to": ["society", "property"],
                }
            )
        if "builder" in scopes:
            traverse.append(
                {
                    "from": "society",
                    "edge": "built_by",
                    "to": "builder",
                    "when": "leaf_scope_includes_builder",
                    "project_facts_to": ["society"],
                }
            )

        assets = ["reddit_resident_facts", "google_review_facts", "rera_legal_facts"]
        if surface_id == "approach_road":
            assets.extend(["google_nearby_place_facts", "image_media_facts"])
        if surface_id in ("litigation_legal", "home_state"):
            assets = ["rera_legal_facts", "reddit_resident_facts"]

        targets.append(
            {
                "target_id": surface_id,
                "kind": "surface",
                "label": surface_id.replace("_", " "),
                "leaf_keys": leaf_keys,
                "primary_scopes": scopes,
                "traverse": traverse,
                "assets": sorted(dict.fromkeys(assets)),
                "refresh": "monthly" if surface_id == "approach_road" else "on_change",
                "reddit_enabled": surface_id not in ("litigation_legal", "home_state"),
            }
        )

    for fact in facts:
        if fact["fact_key"].startswith(("risk.approach_road", "legal.", "home_state.")):
            targets.append(
                {
                    "target_id": fact["fact_key"],
                    "kind": "leaf",
                    "label": fact["label"],
                    "leaf_keys": [fact["fact_key"]],
                    "primary_scopes": fact["scopes"],
                    "traverse": [
                        {
                            "from": "society",
                            "edge": "served_by_road",
                            "to": "road_segment",
                            "when": "road_segment" in fact["scopes"],
                            "project_facts_to": ["society", "property"],
                        }
                    ]
                    if "road_segment" in fact["scopes"]
                    else [],
                    "assets": ["reddit_resident_facts", "google_review_facts"],
                    "refresh": "monthly",
                    "enrichment_terms": fact.get("enrichment_terms", []),
                }
            )

    return {
        "version": 1,
        "_comment": "RE-ENRICHMENT PLANS — run by leaf or UI surface. Traverse society→road→area; project facts down to properties.",
        "description": "Leaf- and surface-scoped enrichment plans. Run backward: leaf → traverse graph → enrich shared nodes → project down.",
        "commands": {
            "by_leaf": "openestates-enrich --leaf <fact_key>",
            "by_surface": "openestates-enrich --surface <surface_id>",
            "by_entity": "openestates-enrich --entity <entity_id>",
        },
        "targets": targets,
        "target_count": len(targets),
    }


def build_ui_surfaces(facts: List[Dict[str, Any]]) -> Dict[str, Any]:
    surface_defs = [
        {
            "id": "approach_road",
            "title": "Approach road",
            "kicker": "Access proof",
            "leaf_keys": leaf_keys_for_surface("approach_road", facts),
            "traversal": ["property → in_society → society → served_by_road → road_segment"],
            "components": ["PropertyArrivalFilm", "livability_brief:risk", "search_chip:approach_road"],
            "primary_entity": "road_segment",
        },
        {
            "id": "water_supply",
            "title": "Water supply",
            "kicker": "Operating proof",
            "leaf_keys": leaf_keys_for_surface("water_utilities", facts),
            "traversal": ["property → in_society → society", "society → in_area → area"],
            "components": ["livability_brief:operating", "search_chip:water_issues"],
            "primary_entity": "society",
        },
        {
            "id": "flooding",
            "title": "Flooding & drainage",
            "kicker": "Risk proof",
            "leaf_keys": leaf_keys_for_surface("flooding_drainage", facts),
            "traversal": ["property → in_society → society", "society → served_by_road → road_segment"],
            "components": ["livability_brief:risk", "search_chip:waterlogging"],
            "primary_entity": "road_segment",
        },
        {
            "id": "legal_rera",
            "title": "Legal & RERA",
            "kicker": "Regulatory proof",
            "leaf_keys": leaf_keys_for_surface("litigation_legal", facts),
            "traversal": ["property → in_society → society → built_by → builder"],
            "components": ["EvidenceStack:legal", "TrustBadge", "search_chip:legal_safety"],
            "primary_entity": "society",
        },
        {
            "id": "home_state",
            "title": "Delivery & OC",
            "kicker": "Lifecycle proof",
            "leaf_keys": leaf_keys_for_surface("home_state", facts),
            "traversal": ["property → in_society → society"],
            "components": ["ProjectStatusTag", "livability_brief:lifecycle", "search_chip:delivered"],
            "primary_entity": "society",
        },
        {
            "id": "livability_positive",
            "title": "Living positives",
            "kicker": "Resident proof",
            "leaf_keys": leaf_keys_for_surface("livability_positive", facts),
            "traversal": ["property → in_society → society"],
            "components": ["livability_brief:positive", "CommunityPulse"],
            "primary_entity": "society",
        },
    ]
    return {
        "version": 1,
        "_comment": "UI SURFACES — maps buyer-facing sections to leaf_keys and React components. One signal, one primary surface.",
        "description": "Buyer-facing surfaces map to leaf sets. One signal, one primary surface.",
        "surfaces": surface_defs,
        "surface_count": len(surface_defs),
    }


def main() -> None:
    fact_registry = build_fact_registry()
    write_json(DAG / "fact_registry.json", fact_registry)

    enrichment = build_enrichment_targets(fact_registry["facts"])
    write_json(DAG / "enrichment_targets.json", enrichment)

    ui_surfaces = build_ui_surfaces(fact_registry["facts"])
    write_json(DAG / "ui_surfaces.json", ui_surfaces)

    manifest = load_json(DAG / "manifest.json")
    manifest["includes"] = sorted(
        dict.fromkeys(
            manifest.get("includes", [])
            + ["fact_registry.json", "enrichment_targets.json", "ui_surfaces.json"]
        )
    )
    manifest["pending"] = [p for p in manifest.get("pending", []) if p not in manifest["includes"]]
    manifest["agent_routing"] = {
        **manifest.get("agent_routing", {}),
        "add_leaf": "concern_taxonomy.json + fact_registry.json",
        "enrich_leaf": "enrichment_targets.json",
        "add_ui_surface": "ui_surfaces.json",
        "bootstrap_policy": "app/config/bootstrap/",
        "bootstrap_edge_inference": "app/config/bootstrap/edge_inference.json",
    }
    write_json(DAG / "manifest.json", manifest)

    print(f"Wrote fact_registry.json ({fact_registry['fact_count']} facts)")
    print(f"Wrote enrichment_targets.json ({enrichment['target_count']} targets)")
    print(f"Wrote ui_surfaces.json ({ui_surfaces['surface_count']} surfaces)")


if __name__ == "__main__":
    main()
