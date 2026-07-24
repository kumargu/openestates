import unittest

from pipeline.skills.fetch_rera import (
    ReraProjectDetail,
    ReraSearchResult,
    parse_rera_detail,
    rera_detail_to_facts,
)


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

    def test_parse_project_details_when_rera_uses_menu1(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu1" class="tab-pane">
          <p class="text-right">Project Type<span>:</span></p>
          <p>Residential/Group Housing</p>
          <p class="text-right">Project Address<span>:</span></p>
          <p>Khata No. 1386, Sy. No. 123, Pattandur Agrahara Village</p>
          <p class="text-right">District<span>:</span></p>
          <p>Bengaluru Urban</p>
        </div>
        <div id="menu2" class="tab-pane">Uploaded Documents</div>
        """
        search_result = ReraSearchResult(
            ack_number="ACK-1",
            registration_number="PRM-1",
            promoter_name="Prestige",
            project_name="Prestige Waterford",
            status="Registered",
            district="Bengaluru Urban",
            taluk="Bengaluru East",
            project_type="Residential",
            approved_on="",
            completion_date="",
            original_completion_date="",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)

        self.assertEqual(detail.project_type, "Residential/Group Housing")
        self.assertEqual(
            detail.project_address,
            "Khata No. 1386, Sy. No. 123, Pattandur Agrahara Village",
        )


if __name__ == "__main__":
    unittest.main()
