"""
batch_runner — Run a skill across all societies or properties with checkpointing.

Features:
- Sequential execution with checkpoint/resume
- Graceful KeyboardInterrupt handling
- Structured JSON report after batch completes
- CLI with --all-societies, --all-properties, --resume, --dry-run

Usage:
    python3 -m pipeline.skills.batch_runner learn_society --all-societies
    python3 -m pipeline.skills.batch_runner search_reddit --all-societies --dry-run
    python3 -m pipeline.skills.batch_runner learn_society --all-societies --resume
    python3 -m pipeline.skills.batch_runner fetch_images --all-societies --force
"""

import argparse
import hashlib
import json
import logging
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Type

from pipeline.skills.base import BaseSkill, SkillExecutionError, SkillResult

logger = logging.getLogger(__name__)

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
CHECKPOINT_DIR = PROJECT_ROOT / "data" / "cache" / "skills"
SEED_DIR = PROJECT_ROOT / "data" / "seed"
KNOWLEDGE_DIR = PROJECT_ROOT / "data" / "knowledge" / "nodes"


def _checkpoint_hash(skill_id: str, input_keys: List[str]) -> str:
    """Deterministic hash for a batch run."""
    raw = f"{skill_id}|{'|'.join(sorted(input_keys))}"
    return hashlib.sha256(raw.encode()).hexdigest()[:12]


class BatchCheckpoint:
    """Persistent checkpoint for a batch run."""

    def __init__(self, skill_id: str, input_keys: List[str]):
        self.skill_id = skill_id
        self.batch_hash = _checkpoint_hash(skill_id, input_keys)
        CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
        self.path = CHECKPOINT_DIR / f"batch_{skill_id}_{self.batch_hash}.json"
        self.completed: List[str] = []
        self.failed: Dict[str, str] = {}  # key -> error message
        self.started_at: str = ""
        self.last_updated: str = ""
        self.total_cost_usd: float = 0.0

    def load(self) -> bool:
        """Load checkpoint from disk. Returns True if loaded."""
        if not self.path.exists():
            return False
        try:
            data = json.loads(self.path.read_text())
            self.completed = data.get("completed", [])
            self.failed = data.get("failed", {})
            self.started_at = data.get("started_at", "")
            self.last_updated = data.get("last_updated", "")
            self.total_cost_usd = data.get("total_cost_usd", 0.0)
            return True
        except (json.JSONDecodeError, KeyError) as e:
            logger.warning("Corrupt checkpoint %s: %s — starting fresh", self.path, e)
            return False

    def save(self) -> None:
        """Write checkpoint to disk (atomic via .tmp + rename)."""
        self.last_updated = datetime.now(timezone.utc).isoformat()
        data = {
            "skill_id": self.skill_id,
            "batch_hash": self.batch_hash,
            "completed": self.completed,
            "failed": self.failed,
            "started_at": self.started_at,
            "last_updated": self.last_updated,
            "total_cost_usd": self.total_cost_usd,
        }
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(data, indent=2))
        tmp.rename(self.path)

    def mark_completed(self, key: str, cost_usd: float = 0.0) -> None:
        self.completed.append(key)
        self.total_cost_usd += cost_usd
        # Remove from failed if it was previously failed (on resume+retry)
        self.failed.pop(key, None)

    def mark_failed(self, key: str, error: str) -> None:
        self.failed[key] = error

    def is_done(self, key: str) -> bool:
        return key in self.completed


class BatchRunner:
    """Run a skill across a list of inputs with checkpointing and error handling."""

    def __init__(self, skill: BaseSkill, inputs: Dict[str, dict], resume: bool = False,
                 force: bool = False, max_retries: Optional[int] = None):
        """
        Args:
            skill: The skill instance to run.
            inputs: Dict of key -> input_data. Key is used for checkpoint tracking.
            resume: If True, load existing checkpoint and skip completed items.
            force: Skip skill cache (passed to skill.run).
            max_retries: Override retry count for each skill.run call.
        """
        self.skill = skill
        self.inputs = inputs
        self.force = force
        self.max_retries = max_retries

        self.checkpoint = BatchCheckpoint(skill.skill_id, list(inputs.keys()))
        if resume:
            loaded = self.checkpoint.load()
            if loaded:
                logger.info(
                    "Resumed checkpoint: %d completed, %d failed",
                    len(self.checkpoint.completed), len(self.checkpoint.failed),
                )
            else:
                logger.info("No checkpoint found — starting fresh")

        if not self.checkpoint.started_at:
            self.checkpoint.started_at = datetime.now(timezone.utc).isoformat()

    def run(self, dry_run: bool = False) -> Dict[str, Any]:
        """
        Execute the batch. Returns a report dict.

        On KeyboardInterrupt, saves checkpoint and returns partial report.
        """
        total = len(self.inputs)
        skipped = 0
        succeeded = 0
        failed = 0
        start_time = time.time()
        interrupted = False

        keys = list(self.inputs.keys())

        try:
            for idx, key in enumerate(keys, 1):
                if self.checkpoint.is_done(key):
                    skipped += 1
                    continue

                input_data = self.inputs[key]
                label = input_data.get("society_name") or input_data.get("query") or key

                if dry_run:
                    print(f"  [{idx}/{total}] Would run {self.skill.skill_id} on: {label}")
                    continue

                print(f"  [{idx}/{total}] {self.skill.skill_id} -> {label}", end="", flush=True)

                try:
                    result = self.skill.run(
                        input_data,
                        force=self.force,
                        max_retries=self.max_retries,
                    )
                    cost = result.cost.estimated_usd
                    self.checkpoint.mark_completed(key, cost)
                    succeeded += 1
                    cached_tag = " [cached]" if result.cached else ""
                    print(f"  OK ({len(result.facts)} facts, ${cost:.4f}){cached_tag}")

                except SkillExecutionError as e:
                    self.checkpoint.mark_failed(key, str(e.original_error))
                    failed += 1
                    print(f"  FAILED ({e.attempts} attempts): {e.original_error}")

                except Exception as e:
                    self.checkpoint.mark_failed(key, str(e))
                    failed += 1
                    print(f"  ERROR: {e}")

                # Save checkpoint after each item
                self.checkpoint.save()

        except KeyboardInterrupt:
            interrupted = True
            print(f"\n\n  Interrupted! Saving checkpoint...")
            self.checkpoint.save()
            print(f"  Checkpoint saved: {self.checkpoint.path}")

        elapsed = time.time() - start_time

        report = self._build_report(
            total=total,
            succeeded=succeeded,
            failed=failed,
            skipped=skipped,
            elapsed=elapsed,
            interrupted=interrupted,
            dry_run=dry_run,
        )

        if not dry_run:
            self._save_report(report)
            self._print_summary(report)

        return report

    def _build_report(self, total: int, succeeded: int, failed: int,
                      skipped: int, elapsed: float, interrupted: bool,
                      dry_run: bool) -> Dict[str, Any]:
        return {
            "skill_id": self.skill.skill_id,
            "total": total,
            "succeeded": succeeded,
            "failed": failed,
            "skipped": skipped,
            "pending": total - succeeded - failed - skipped,
            "total_cost_usd": self.checkpoint.total_cost_usd,
            "elapsed_seconds": round(elapsed, 1),
            "interrupted": interrupted,
            "dry_run": dry_run,
            "failed_items": dict(self.checkpoint.failed),
            "started_at": self.checkpoint.started_at,
            "completed_at": datetime.now(timezone.utc).isoformat(),
            "checkpoint_path": str(self.checkpoint.path),
        }

    def _save_report(self, report: Dict[str, Any]) -> None:
        """Write structured report to disk."""
        CHECKPOINT_DIR.mkdir(parents=True, exist_ok=True)
        ts = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        report_path = CHECKPOINT_DIR / f"batch_report_{self.skill.skill_id}_{ts}.json"
        report_path.write_text(json.dumps(report, indent=2))
        logger.info("Report written to %s", report_path)

    def _print_summary(self, report: Dict[str, Any]) -> None:
        """Print human-readable summary."""
        print(f"\n{'='*60}")
        print(f"  Batch Report: {report['skill_id']}")
        print(f"{'='*60}")
        print(f"  Total items:   {report['total']}")
        print(f"  Succeeded:     {report['succeeded']}")
        print(f"  Failed:        {report['failed']}")
        print(f"  Skipped:       {report['skipped']}")
        print(f"  Pending:       {report['pending']}")
        print(f"  Total cost:    ${report['total_cost_usd']:.4f}")
        print(f"  Elapsed:       {report['elapsed_seconds']:.1f}s")
        if report['interrupted']:
            print(f"  Status:        INTERRUPTED (resume with --resume)")
        elif report['failed'] > 0:
            print(f"  Status:        PARTIAL (retry failed with --resume)")
        else:
            print(f"  Status:        COMPLETE")

        if report['failed_items']:
            print(f"\n  Failed items:")
            for key, err in report['failed_items'].items():
                print(f"    - {key}: {err[:100]}")

        print(f"\n  Checkpoint: {report['checkpoint_path']}")
        print(f"{'='*60}\n")


# ──────────────────────────────────────────────────────────────
# Input generators: load entities from seed/knowledge data
# ──────────────────────────────────────────────────────────────

def load_society_inputs(skill_id: str) -> Dict[str, dict]:
    """Load all societies and build input dicts appropriate for the skill."""
    inputs: Dict[str, dict] = {}

    # Try knowledge graph nodes first, then seed data
    kg_dir = KNOWLEDGE_DIR / "society"
    if kg_dir.exists():
        for f in sorted(kg_dir.glob("*.json")):
            try:
                node = json.loads(f.read_text())
                slug = f.stem
                name = node.get("name", slug.replace("-", " ").title())
                area = node.get("area", "")
                city = node.get("city", "Bengaluru")
                inputs[slug] = _society_input(skill_id, name, area, city, slug)
            except (json.JSONDecodeError, KeyError):
                continue

    # Fallback/supplement from seed data
    seed_path = SEED_DIR / "societies.json"
    if seed_path.exists() and not inputs:
        societies = json.loads(seed_path.read_text())
        for soc in societies:
            slug = soc["id"].replace("soc-", "")
            if slug not in inputs:
                inputs[slug] = _society_input(
                    skill_id,
                    soc["name"],
                    soc.get("area", ""),
                    soc.get("city", "Bengaluru"),
                    slug,
                )

    return inputs


def _society_input(skill_id: str, name: str, area: str, city: str, slug: str) -> dict:
    """Build skill-appropriate input for a society."""
    if skill_id == "learn_society":
        return {"society_name": name, "area": area, "city": city}
    elif skill_id == "search_reddit":
        return {"query": f"{name} {area}", "subreddit": "bangalore"}
    elif skill_id == "score_society":
        return {"society_slug": slug, "society_name": name}
    elif skill_id == "identify_gaps":
        return {"society_slug": slug, "society_name": name}
    elif skill_id == "fetch_images":
        return {"society_name": name, "area": area, "city": city, "slug": slug}
    else:
        # Generic: pass everything
        return {"society_name": name, "area": area, "city": city, "slug": slug}


def load_property_inputs(skill_id: str) -> Dict[str, dict]:
    """Load all properties and build input dicts."""
    inputs: Dict[str, dict] = {}

    # Try knowledge graph nodes first
    kg_dir = KNOWLEDGE_DIR / "property"
    if kg_dir.exists():
        for f in sorted(kg_dir.glob("*.json")):
            try:
                node = json.loads(f.read_text())
                slug = f.stem
                name = node.get("name", slug.replace("-", " ").title())
                area = node.get("area", "")
                city = node.get("city", "Bengaluru")
                inputs[slug] = {"property_name": name, "area": area, "city": city, "slug": slug}
            except (json.JSONDecodeError, KeyError):
                continue

    # Fallback from seed data
    seed_path = SEED_DIR / "properties.json"
    if seed_path.exists() and not inputs:
        properties = json.loads(seed_path.read_text())
        for prop in properties:
            pid = prop.get("id", "")
            slug = pid.replace("prop-", "") if pid else ""
            if slug and slug not in inputs:
                inputs[slug] = {
                    "property_name": prop.get("title", ""),
                    "area": prop.get("area", ""),
                    "city": prop.get("city", "Bengaluru"),
                    "slug": slug,
                    "society_id": prop.get("society_id", ""),
                }

    return inputs


# ──────────────────────────────────────────────────────────────
# Skill registry (import lazily to avoid circular imports)
# ──────────────────────────────────────────────────────────────

def get_skill_instance(skill_id: str) -> BaseSkill:
    """Instantiate a skill by ID."""
    if skill_id == "learn_society":
        from pipeline.skills.learn_society import LearnSocietySkill
        return LearnSocietySkill()
    elif skill_id == "search_reddit":
        from pipeline.skills.search_reddit import SearchRedditSkill
        return SearchRedditSkill()
    elif skill_id == "fetch_images":
        from pipeline.skills.fetch_images import FetchImagesSkill
        # fetch_images doesn't have a BaseSkill subclass, use a wrapper
        raise ValueError(
            "fetch_images has its own __main__ — use: "
            "python3 -m pipeline.skills.fetch_images"
        )
    elif skill_id == "score_society":
        from pipeline.skills.score_society import ScoreSocietySkill
        return ScoreSocietySkill()
    elif skill_id == "identify_gaps":
        from pipeline.skills.identify_gaps import IdentifyGapsSkill
        return IdentifyGapsSkill()
    else:
        raise ValueError(f"Unknown skill: {skill_id}")


AVAILABLE_SKILLS = ["learn_society", "search_reddit", "score_society", "identify_gaps"]


# ──────────────────────────────────────────────────────────────
# CLI
# ──────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Run a skill in batch across all societies or properties",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Examples:
  python3 -m pipeline.skills.batch_runner learn_society --all-societies
  python3 -m pipeline.skills.batch_runner search_reddit --all-societies --dry-run
  python3 -m pipeline.skills.batch_runner learn_society --all-societies --resume
  python3 -m pipeline.skills.batch_runner score_society --all-societies --force
""",
    )
    parser.add_argument("skill", choices=AVAILABLE_SKILLS, help="Skill to run")
    parser.add_argument("--all-societies", action="store_true", help="Run across all societies")
    parser.add_argument("--all-properties", action="store_true", help="Run across all properties")
    parser.add_argument("--resume", action="store_true", help="Resume from checkpoint, retry failed")
    parser.add_argument("--force", action="store_true", help="Skip skill cache")
    parser.add_argument("--dry-run", action="store_true", help="Show what would run without executing")
    parser.add_argument("--retries", type=int, default=None, help="Override max retries per item")

    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(name)s] %(levelname)s: %(message)s",
    )

    if not args.all_societies and not args.all_properties:
        parser.error("Specify --all-societies or --all-properties")

    # Load inputs
    if args.all_societies:
        inputs = load_society_inputs(args.skill)
        entity_type = "societies"
    else:
        inputs = load_property_inputs(args.skill)
        entity_type = "properties"

    if not inputs:
        print(f"  No {entity_type} found in data/knowledge or data/seed")
        sys.exit(1)

    print(f"\n  Batch: {args.skill} x {len(inputs)} {entity_type}")
    if args.resume:
        print(f"  Mode: RESUME (skip completed, retry failed)")
    if args.dry_run:
        print(f"  Mode: DRY RUN")
    print()

    # Instantiate skill
    skill = get_skill_instance(args.skill)

    # Run batch
    runner = BatchRunner(
        skill=skill,
        inputs=inputs,
        resume=args.resume,
        force=args.force,
        max_retries=args.retries,
    )
    report = runner.run(dry_run=args.dry_run)

    # Exit code reflects success
    if report.get("failed", 0) > 0 or report.get("interrupted", False):
        sys.exit(1)


if __name__ == "__main__":
    main()
