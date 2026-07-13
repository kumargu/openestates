"""
Pipeline Orchestrator — backward compatibility wrapper.

DEPRECATED: Use `python3 -m pipeline.enrich` instead.
This module delegates all work to the unified enrichment engine.
"""

import logging
import sys

logger = logging.getLogger(__name__)


def enrich_entity(entity_id, resolver=None, tracker=None, **kwargs):
    """Backward-compatible single-entity enrichment entry point."""
    if entity_id.startswith("society:"):
        return enrich_society(entity_id, resolver=resolver, tracker=tracker, **kwargs)
    if entity_id.startswith("area:"):
        return enrich_area(entity_id, resolver=resolver, tracker=tracker, **kwargs)
    if entity_id.startswith("builder:"):
        return enrich_builder(entity_id, resolver=resolver, tracker=tracker, **kwargs)

    logger.warning("Unsupported entity type for enrichment: %s", entity_id)
    return None


def enrich_society(entity_id, resolver=None, tracker=None, **kwargs):
    """Run deterministic enrichment for one society node."""
    from pipeline.enrich import detect_gaps, execute
    from pipeline.entity_resolver import EntityResolver, FreshnessTracker

    resolver = resolver or EntityResolver()
    tracker = tracker or FreshnessTracker()
    work = detect_gaps(resolver, tracker, node_filter=entity_id, **kwargs)
    return execute(work, resolver, tracker)


def enrich_area(entity_id, resolver=None, tracker=None, **kwargs):
    """Area LLM enrichment was retired; keep a no-op compatibility hook."""
    logger.warning("Area enrichment is not registered for %s", entity_id)
    return None


def enrich_builder(entity_id, resolver=None, tracker=None, **kwargs):
    """Builder enrichment is not registered in the deterministic runner yet."""
    logger.warning("Builder enrichment is not registered for %s", entity_id)
    return None


def main():
    print("WARNING: orchestrate.py is deprecated. Use `python3 -m pipeline.enrich` instead.\n")
    from pipeline.enrich import main as enrich_main
    # Translate old CLI args to new format
    new_args = []
    old_args = sys.argv[1:]
    i = 0
    while i < len(old_args):
        arg = old_args[i]
        if arg == "--entity" and i + 1 < len(old_args):
            new_args.extend(["--node", old_args[i + 1]])
            i += 2
        elif arg == "--list-stale":
            new_args.append("--plan")
            i += 1
        elif arg == "--stale-only":
            # Default behavior in enrich.py (only runs stale items)
            i += 1
        elif arg == "--no-notify":
            # enrich.py doesn't notify by default (needs --reload)
            i += 1
        else:
            new_args.append(arg)
            i += 1

    sys.argv = [sys.argv[0]] + new_args
    enrich_main()


if __name__ == "__main__":
    main()
