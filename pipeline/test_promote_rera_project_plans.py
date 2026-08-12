import json
import tempfile
import unittest
from pathlib import Path

from pipeline.skills.promote_rera_project_plans import (
    ManifestError,
    load_manifest,
    materialize_project,
)


PNG_BYTES = b"\x89PNG\r\n\x1a\nfixture"


def project_manifest(slug: str, bedroom_count: int) -> dict:
    artifact_prefix = f"{slug}:brochure"
    return {
        "society_slug": slug,
        "society_entity_id": f"society:{slug}",
        "provider": "RERA",
        "coverage_quality": "usable",
        "source_url": "https://rera.test/source",
        "registration_number": "PRM-TEST",
        "source_dirs": ["previews"],
        "document_artifacts": [
            {
                "artifact_id": f"{artifact_prefix}:site",
                "kind": "site_plan",
                "label": "Site overview",
                "source_url": "https://rera.test/source",
                "confidence": 0.82,
            },
            {
                "artifact_id": f"{artifact_prefix}:floor",
                "kind": "floor_plan",
                "label": f"{bedroom_count}BHK floor plan",
                "source_url": "https://rera.test/source",
                "configuration_type": f"{bedroom_count}BHK",
                "bedroom_count": bedroom_count,
                "confidence": 0.86,
            },
        ],
        "site_overview": {
            "artifact_id": f"{artifact_prefix}:site",
            "source_name": f"{slug}-site.png",
            "page": 3,
        },
        "floor_plans": [
            {
                "artifact_id": f"{artifact_prefix}:floor",
                "configuration_type": f"{bedroom_count}BHK",
                "bedroom_count": bedroom_count,
                "source_name": f"{slug}-floor.png",
                "carpet_area_sqft": 1000 + bedroom_count,
                "sale_area_sqft": 1500 + bedroom_count,
            }
        ],
    }


class PromoteReraProjectPlansTest(unittest.TestCase):
    def test_materializes_two_manifest_projects_without_hardcoded_society_logic(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            previews = root / "manifest" / "previews"
            previews.mkdir(parents=True)
            for slug in ("prestige-waterford", "godrej-air"):
                (previews / f"{slug}-site.png").write_bytes(PNG_BYTES)
                (previews / f"{slug}-floor.png").write_bytes(PNG_BYTES)

            manifest_path = root / "manifest" / "targets.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "projects": [
                            project_manifest("prestige-waterford", 3),
                            project_manifest("godrej-air", 2),
                        ]
                    }
                ),
                encoding="utf-8",
            )

            results = [
                materialize_project(root, manifest_path, project)
                for project in load_manifest(manifest_path)
            ]

            self.assertEqual([result["floor_plan_count"] for result in results], [1, 1])
            for result in results:
                payload = json.loads(Path(result["fact_path"]).read_text(encoding="utf-8"))
                self.assertEqual(payload["provider"], "RERA")
                self.assertTrue(payload["floor_plans"][0]["preview_url"].startswith("/media/previews/rera_plans/"))
                self.assertEqual(payload["floor_plans"][0]["plan_kind"], "floor_plan")
                self.assertIn("source_hash", payload["floor_plans"][0])

                serving_rows = Path(result["serving_fact_path"]).read_text(encoding="utf-8").splitlines()
                serving = json.loads(serving_rows[0])
                self.assertEqual(serving["fact_key"], "media.project_plan_frames")
                self.assertEqual(serving["value_type"], "text")
                self.assertEqual(serving["source_type"], "Rera")

    def test_materializes_neutral_filed_plan_preview_without_unit_claims(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            previews = root / "manifest" / "previews"
            previews.mkdir(parents=True)
            (previews / "approved-plan-page-2.png").write_bytes(PNG_BYTES)
            project = {
                "society_slug": "example",
                "society_entity_id": "society:example",
                "provider": "RERA",
                "coverage_quality": "filed_plan_preview",
                "source_dirs": ["previews"],
                "document_artifacts": [{
                    "artifact_id": "example:sanction-plan",
                    "kind": "sanction_plan",
                    "label": "Approved basement plan",
                    "source_url": "https://rera.test/approved-plan",
                    "confidence": 0.82,
                }],
                "filed_plan_previews": [{
                    "artifact_id": "example:sanction-plan",
                    "source_name": "approved-plan-page-2.png",
                    "page": 2,
                }],
                "floor_plans": [],
            }

            result = materialize_project(root, root / "manifest" / "targets.json", project)
            payload = json.loads(Path(result["fact_path"]).read_text(encoding="utf-8"))

            self.assertEqual(result["filed_plan_preview_count"], 1)
            self.assertEqual(result["floor_plan_count"], 0)
            self.assertEqual(payload["filed_plan_previews"][0]["kind"], "sanction_plan")
            self.assertNotIn("configuration_type", payload["filed_plan_previews"][0])

    def test_materializes_empty_gap_target_without_preview_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            project = {
                "society_slug": "godrej-air-nxt",
                "society_entity_id": "society:godrej-air-nxt",
                "provider": "RERA",
                "coverage_quality": "rera_detail_not_cached",
                "source_url": "https://rera.test/source",
                "registration_number": "PRM-GAP",
                "document_artifacts": [],
                "floor_plans": [],
            }

            result = materialize_project(root, root / "manifest" / "targets.json", project)
            payload = json.loads(Path(result["fact_path"]).read_text(encoding="utf-8"))

            self.assertEqual(result["floor_plan_count"], 0)
            self.assertEqual(payload["provider"], "RERA")
            self.assertNotIn("site_overview", payload)
            self.assertEqual(payload["floor_plans"], [])

    def test_rejects_floor_plan_not_backed_by_rera_document_artifact(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            previews = root / "manifest" / "previews"
            previews.mkdir(parents=True)
            (previews / "prestige-waterford-site.png").write_bytes(PNG_BYTES)
            (previews / "prestige-waterford-floor.png").write_bytes(PNG_BYTES)
            project = project_manifest("prestige-waterford", 3)
            project["floor_plans"][0]["artifact_id"] = "prestige-waterford:missing"

            with self.assertRaises(ManifestError):
                materialize_project(root, root / "manifest" / "targets.json", project)


if __name__ == "__main__":
    unittest.main()
