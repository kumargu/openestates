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


if __name__ == "__main__":
    unittest.main()
