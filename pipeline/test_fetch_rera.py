import unittest

from pipeline.skills.fetch_rera import ReraProjectDetail, rera_detail_to_facts


class FetchReraSkillTest(unittest.TestCase):
    def test_complaints_resolved_percentage_is_capped_at_100(self):
        detail = ReraProjectDetail(
            complaints_count=2,
            complaints_resolved=4,
        )

        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertEqual(
            facts["rera_complaints_resolved_pct"].value,
            {"type": "Numeric", "data": 100.0},
        )

    def test_location_emits_legacy_and_structured_coordinate_facts(self):
        detail = ReraProjectDetail(latitude="12.9698", longitude="77.75")

        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertEqual(
            facts["rera_lat_lng"].value,
            {"type": "Text", "data": "12.9698,77.75"},
        )
        self.assertEqual(
            facts["geo.latitude"].value,
            {"type": "Numeric", "data": 12.9698},
        )
        self.assertEqual(
            facts["geo.longitude"].value,
            {"type": "Numeric", "data": 77.75},
        )


if __name__ == "__main__":
    unittest.main()
