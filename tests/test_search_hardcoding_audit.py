import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
AUDIT_SCRIPT = REPO_ROOT / "scripts" / "audit_search_hardcoding.py"


class SearchHardcodingAuditTests(unittest.TestCase):
    def test_gate_rejects_product_fact_key_comparison(self) -> None:
        source = (
            'facts.iter().find(|fact| '
            'fact.fact_key.eq_ignore_ascii_case("builder_delivery_rate"));\n'
        )
        result = self._run_fixture(source, ["builder_delivery_rate"])

        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("hardcoded_fact_key? `builder_delivery_rate`", result.stdout)

    def test_gate_allows_structural_fact_key_comparison(self) -> None:
        source = (
            'facts.iter().find(|fact| '
            'fact.fact_key.eq_ignore_ascii_case("place.category"));\n'
        )
        result = self._run_fixture(source, ["place.category"])

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Fact-key comparison findings: 0", result.stdout)

    def test_gate_allows_generic_configured_comparison(self) -> None:
        source = "facts.iter().find(|fact| fact.fact_key.eq_ignore_ascii_case(configured_key));\n"
        result = self._run_fixture(source, ["builder_delivery_rate"])

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Fact-key comparison findings: 0", result.stdout)

    def _run_fixture(self, source: str, fact_keys: list[str]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            dag = root / "app" / "config" / "dag"
            search = root / "backend" / "src" / "search"
            dag.mkdir(parents=True)
            search.mkdir(parents=True)
            (dag / "search_intent.json").write_text(
                json.dumps({"resolution": {"place_families": []}}),
                encoding="utf-8",
            )
            (dag / "nearby_place_categories.json").write_text(
                json.dumps(
                    {
                        "categories": [
                            {
                                "category_aliases": ["hospital"],
                                "answers_preferences": [],
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            (dag / "fact_registry.json").write_text(
                json.dumps({"facts": [{"fact_key": key} for key in fact_keys]}),
                encoding="utf-8",
            )
            (search / "sample.rs").write_text(source, encoding="utf-8")
            return subprocess.run(
                [
                    sys.executable,
                    str(AUDIT_SCRIPT),
                    "--root",
                    str(root),
                    "--mode",
                    "production-search",
                    "--gate",
                ],
                check=False,
                capture_output=True,
                text=True,
            )


if __name__ == "__main__":
    unittest.main()
