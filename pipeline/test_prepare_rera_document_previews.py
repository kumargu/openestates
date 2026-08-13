import unittest

from pipeline.skills.prepare_rera_document_previews import (
    _official_document_url,
    _render_rejection_reason,
)
from pipeline.skills.rera_document_intelligence import load_document_policy


class PrepareReraDocumentPreviewsTest(unittest.TestCase):
    def setUp(self):
        self.policy = load_document_policy()

    def test_accepts_plan_like_render(self):
        signals = {
            "dark_ratio": 0.199025,
            "mid_tone_ratio": 0.198533,
            "very_dark_ratio": 0.000492,
            "edge_ratio": 0.138452,
        }

        self.assertIsNone(_render_rejection_reason(signals, self.policy))

    def test_rejects_blank_render(self):
        signals = {
            "dark_ratio": 0.0,
            "mid_tone_ratio": 0.0,
            "very_dark_ratio": 0.0,
            "edge_ratio": 0.0,
        }

        self.assertEqual(
            _render_rejection_reason(signals, self.policy),
            "blank_render",
        )

    def test_rejects_dense_photo_like_render(self):
        signals = {
            "dark_ratio": 0.5,
            "mid_tone_ratio": 0.7,
            "very_dark_ratio": 0.3,
            "edge_ratio": 0.2,
        }

        self.assertEqual(
            _render_rejection_reason(signals, self.policy),
            "photo_or_dense_render",
        )

    def test_normalizes_relative_rera_document_links(self):
        self.assertEqual(
            _official_document_url("download_jc?DOC_ID=example"),
            "https://rera.karnataka.gov.in/download_jc?DOC_ID=example",
        )


if __name__ == "__main__":
    unittest.main()
