"""Tests for enrichment gap → target priority stub."""

import json
import tempfile
import unittest
from pathlib import Path

from pipeline.enrichment_priority import build_priority_queue


class EnrichmentPriorityTests(unittest.TestCase):
    def test_maps_missing_fact_to_target_id(self):
        targets_doc = {
            "targets": [
                {
                    "target_id": "approach_road",
                    "leaf_keys": ["risk.approach_road_waterlogging"],
                }
            ]
        }
        gaps = [
            {
                "entity_id": "society:prestige-park-grove",
                "missing_fact": "risk.approach_road_waterlogging",
            }
        ]
        queue = build_priority_queue(gaps, targets_doc)
        self.assertEqual(len(queue), 1)
        self.assertEqual(queue[0]["target_id"], "approach_road")
        self.assertEqual(queue[0]["entity_id"], "society:prestige-park-grove")

    def test_m7_sentinel_gap_keys_have_target_mappings(self):
        targets_doc = {
            "targets": [
                {
                    "target_id": "water_utilities",
                    "leaf_keys": ["operating.tanker_dependence", "water_supply_risk"],
                },
                {
                    "target_id": "livability_positive",
                    "leaf_keys": ["maintenance_sentiment", "positive.maintenance_quality"],
                },
                {
                    "target_id": "flooding_drainage",
                    "leaf_keys": ["waterlogging_risk_score"],
                },
                {
                    "target_id": "approach_road",
                    "leaf_keys": ["risk.approach_road_waterlogging"],
                },
                {
                    "target_id": "litigation_legal",
                    "leaf_keys": [
                        "bbmp_approval_status",
                        "occupancy_certificate_status",
                        "lifecycle.builder_reputation_negative",
                    ],
                },
            ]
        }
        sentinel_keys = [
            "operating.tanker_dependence",
            "water_supply_risk",
            "maintenance_sentiment",
            "positive.maintenance_quality",
            "waterlogging_risk_score",
            "risk.approach_road_waterlogging",
            "bbmp_approval_status",
            "occupancy_certificate_status",
            "lifecycle.builder_reputation_negative",
        ]
        gaps = [
            {
                "entity_id": "society:test",
                "missing_fact": missing_fact,
            }
            for missing_fact in sentinel_keys
        ]

        queue = build_priority_queue(gaps, targets_doc)
        mapped_facts = {item["missing_fact"] for item in queue}

        self.assertEqual(mapped_facts, set(sentinel_keys))

    def test_cli_writes_output_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            gaps_path = Path(tmp) / "gaps.json"
            targets_path = Path(tmp) / "targets.json"
            output_path = Path(tmp) / "queue.json"
            gaps_path.write_text(
                json.dumps(
                    [
                        {
                            "entity_id": "society:test",
                            "missing_fact": "risk.approach_road_waterlogging",
                        }
                    ]
                ),
                encoding="utf-8",
            )
            targets_path.write_text(
                json.dumps(
                    {
                        "targets": [
                            {
                                "target_id": "approach_road",
                                "leaf_keys": ["risk.approach_road_waterlogging"],
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            import subprocess
            import sys

            subprocess.check_call(
                [
                    sys.executable,
                    "-m",
                    "pipeline.enrichment_priority",
                    "--gaps",
                    str(gaps_path),
                    "--targets",
                    str(targets_path),
                    "--output",
                    str(output_path),
                ]
            )
            payload = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(len(payload), 1)


if __name__ == "__main__":
    unittest.main()
