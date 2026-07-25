import unittest

from pipeline.skills.fetch_rera import (
    ReraProjectDetail,
    ReraSearchResult,
    parse_rera_detail,
    rera_detail_to_facts,
)


class FetchReraSkillTest(unittest.TestCase):
    def test_rera_cost_fields_are_not_promoted(self):
        detail = ReraProjectDetail(
            total_project_cost_inr=500_000_000,
            land_cost_inr=200_000_000,
            construction_cost_inr=300_000_000,
            total_units=100,
        )

        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertNotIn("rera_total_project_cost", facts)
        self.assertNotIn("rera_land_cost", facts)
        self.assertNotIn("rera_construction_cost", facts)
        self.assertNotIn("rera_cost_per_unit", facts)

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

    def test_parse_plan_documents_into_configurations_and_media_facts(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu1" class="tab-pane">
          Project Type : Residential
          Total Number of Inventories/Flats/Villas : 689
          Number of Towers : 7
          Total Area Of Land (Sq Mtr) : 66813
          Total Open Area : 59376
          Latitude : 12°58'11.20"N
          Longitude : 77°44'10.80"E
          <a href="/document/site-plan.pdf">Overall Site Plan</a>
          <a href="/document/2bhk-floor-plan.pdf">2 BHK Floor Plan</a>
          <a href="/document/3bhk-floor-plan.pdf">3 BHK Floor Plan</a>
          <a href="/document/sanction-plan.pdf">Sanction Plan</a>
        </div>
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
            completion_date="31-12-2027",
            original_completion_date="31-12-2025",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertEqual(detail.latitude, "12.969778")
        self.assertEqual(detail.longitude, "77.736333")
        self.assertEqual(facts["project_unit_count"].value, {"type": "Numeric", "data": 689})
        self.assertEqual(facts["project_tower_count"].value, {"type": "Numeric", "data": 7})
        self.assertEqual(facts["site_plan_asset_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["floor_plan_asset_count"].value, {"type": "Numeric", "data": 2})
        self.assertEqual(
            facts["available_configurations"].value,
            {"type": "Tags", "data": ["2BHK", "3BHK"]},
        )
        self.assertEqual(facts["has_2bhk"].value, {"type": "Bool", "data": True})
        self.assertEqual(facts["has_3bhk"].value, {"type": "Bool", "data": True})


if __name__ == "__main__":
    unittest.main()
