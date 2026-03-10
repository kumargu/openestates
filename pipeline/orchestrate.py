"""
Pipeline Orchestrator — backward compatibility wrapper.

DEPRECATED: Use `python3 -m pipeline.enrich` instead.
This module delegates all work to the unified enrichment engine.
"""

import logging
import sys

logger = logging.getLogger(__name__)


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
