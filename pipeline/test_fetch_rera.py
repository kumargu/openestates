import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from pipeline.skills.fetch_rera import (
    ReraDocumentArtifact,
    ReraProjectDetail,
    ReraSearchResult,
    classify_rera_document,
    parse_rera_detail,
    prepare_rera_plan_previews,
    rera_detail_to_facts,
    scrape_rera_listing,
    search_rera_registration,
    validate_rera_plan_preview,
)


class FetchReraSkillTest(unittest.TestCase):
    @staticmethod
    def plan_artifact(label, source_field_label=None):
        classification = classify_rera_document(label, source_field_label)
        return ReraDocumentArtifact(
            artifact_id="artifact:" + label.lower().replace(" ", "-"),
            document_kind=classification["kind"],
            label=label,
            source_url="https://rera.test/" + label.replace(" ", "%20"),
            source_field_label=source_field_label,
            document_group=classification["group"],
            buyer_visibility=classification["buyer_visibility"],
            preview_policy=classification["preview_policy"],
            preview_role=classification["preview_role"],
            buyer_label=classification["buyer_label"],
        )

    def test_document_policy_accepts_named_plans_and_excludes_forbidden_categories(self):
        accepted = {
            "9th Floor Plan1.pdf": ("floor_plan", "floor_plan"),
            "siteplan.pdf": ("site_plan", "site_plan"),
            "Approved Plans.pdf": ("sanction_plan", "sanction_plan"),
        }
        for label, expected in accepted.items():
            with self.subTest(label=label):
                classification = classify_rera_document(label)
                self.assertEqual(
                    (classification["kind"], classification["preview_role"]),
                    expected,
                )
                self.assertEqual(classification["preview_policy"], "auto_validate")

        excluded = (
            "Project Photos.pdf",
            "Project Brochure.pdf",
            "Affidavit Annexure 49.pdf",
            "Auditor Balance Sheet.pdf",
            "Agreement for Sale.pdf",
        )
        for label in excluded:
            with self.subTest(label=label):
                classification = classify_rera_document(label)
                self.assertEqual(classification["preview_policy"], "hidden")
                self.assertIsNone(classification["buyer_label"])

    def test_registration_lookup_posts_exact_registration_and_ignores_other_rows(self):
        def row(numeric_id, registration, project):
            cells = [
                "1",
                "ACK",
                registration,
                "",
                "Builder",
                project,
                "Registered",
                "Bengaluru Urban",
                "Bengaluru East",
                "Residential",
                "01-01-2024",
                "01-01-2028",
                "01-01-2028",
            ]
            return "<tr><a id=\"{}\" onclick=\"return showFileApplicationPreview\"></a>{}</tr>".format(
                numeric_id,
                "".join("<td>{}</td>".format(value) for value in cells),
            )

        class FakeSession:
            request = None

            def post(self, _url, data):
                self.request = data
                return row("11", "PRM-OTHER", "Other") + row("22", "PRM-EXACT", "Exact")

        session = FakeSession()
        result = search_rera_registration(session, "PRM-EXACT")

        self.assertEqual(session.request["regNo"], "PRM-EXACT")
        self.assertEqual(session.request["project"], "")
        self.assertEqual(result.registration_number, "PRM-EXACT")
        self.assertEqual(result.numeric_id, "22")

    def test_preview_validation_rejects_blank_photo_legal_financial_and_sensitive_pages(self):
        drawing = {
            "dark_ratio": 0.08,
            "mid_tone_ratio": 0.05,
            "very_dark_ratio": 0.03,
            "edge_ratio": 0.08,
        }
        accepted, reason = validate_rera_plan_preview(
            {"label": "Approved plan.pdf"}, "TYPICAL FLOOR PLAN", drawing
        )
        self.assertEqual((accepted, reason), ("accepted", None))

        cases = (
            ({"label": "Approved plan.pdf"}, "", {**drawing, "dark_ratio": 0.0}, "blank_render"),
            ({"label": "Project Photos.pdf"}, "", drawing, "photo_document"),
            ({"label": "Project Brochure.pdf"}, "", drawing, "cover_or_brochure"),
            ({"label": "Approved plan.pdf"}, "Affidavit and sale deed", drawing, "legal_document"),
            ({"label": "Approved plan.pdf"}, "Bank Account and balance sheet", drawing, "private_or_sensitive"),
            ({"label": "Approved plan.pdf"}, "Digitally signed by architect", drawing, "private_or_sensitive"),
        )
        for artifact, text, signals, expected_reason in cases:
            with self.subTest(reason=expected_reason):
                status, reason = validate_rera_plan_preview(artifact, text, signals)
                self.assertEqual((status, reason), ("rejected", expected_reason))

    def test_scanned_preview_uses_ocr_and_preview_runs_are_deterministic(self):
        artifacts = [
            self.plan_artifact("Approved Plans.pdf"),
            self.plan_artifact("9th Floor Plan1.pdf"),
            self.plan_artifact("siteplan.pdf"),
        ]
        drawing = {
            "dark_ratio": 0.08,
            "mid_tone_ratio": 0.05,
            "very_dark_ratio": 0.03,
            "edge_ratio": 0.08,
        }

        def download(url):
            return b"%PDF-1.4\n" + url.encode("utf-8")

        def render(pdf_path, output_path, _size):
            output_path.write_bytes(b"\x89PNG\r\n\x1a\n" + pdf_path.stem.encode("ascii"))

        with tempfile.TemporaryDirectory() as temp_dir:
            kwargs = {
                "artifacts": artifacts,
                "registration_number": "PRM-TEST",
                "cache_root": Path(temp_dir),
                "download": download,
                "render": render,
                "extract_text": lambda _path: "",
                "ocr_text": lambda _path: "SCANNED TYPICAL FLOOR PLAN",
                "analyze_image": lambda _path, _size: drawing,
            }
            first = prepare_rera_plan_previews(**kwargs)
            second = prepare_rera_plan_previews(**kwargs)

        self.assertEqual(first["previews"], second["previews"])
        self.assertEqual(first["payload_hash"], second["payload_hash"])
        self.assertEqual(
            [preview["preview_hash"] for preview in first["previews"]],
            [preview["preview_hash"] for preview in second["previews"]],
        )
        self.assertEqual(
            [preview["buyer_label"] for preview in first["previews"]],
            ["Site plan", "Floor plan", "Approved plan"],
        )
        self.assertTrue(all(preview["text_method"] == "tesseract" for preview in first["previews"]))

    def test_preview_selection_deduplicates_identical_rendered_pages(self):
        artifacts = [
            self.plan_artifact("siteplan.pdf"),
            self.plan_artifact("Approved Plans.pdf"),
        ]
        drawing = {
            "dark_ratio": 0.08,
            "mid_tone_ratio": 0.05,
            "very_dark_ratio": 0.03,
            "edge_ratio": 0.08,
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            result = prepare_rera_plan_previews(
                artifacts,
                "PRM-DUPLICATE-PREVIEW",
                cache_root=Path(temp_dir),
                download=lambda url: b"%PDF-1.4\n" + url.encode("utf-8"),
                render=lambda _pdf, output, _size: output.write_bytes(
                    b"\x89PNG\r\n\x1a\nidentical-render"
                ),
                extract_text=lambda _path: "TYPICAL PLAN",
                ocr_text=lambda _path: "",
                analyze_image=lambda _path, _size: drawing,
            )

        self.assertEqual([preview["buyer_label"] for preview in result["previews"]], ["Site plan"])
        duplicate = next(
            review
            for review in result["document_reviews"]
            if review["buyer_label"] == "Approved plan"
        )
        self.assertEqual(duplicate["rejection_reason"], "duplicate_preview")

    def test_sensitive_source_field_rejects_an_eligible_filename(self):
        artifact = self.plan_artifact("Approved Plans.pdf", "Balance Sheet")
        drawing = {
            "dark_ratio": 0.08,
            "mid_tone_ratio": 0.05,
            "very_dark_ratio": 0.03,
            "edge_ratio": 0.08,
        }

        with tempfile.TemporaryDirectory() as temp_dir:
            result = prepare_rera_plan_previews(
                [artifact],
                "PRM-SENSITIVE",
                cache_root=Path(temp_dir),
                download=lambda _url: b"%PDF-1.4\nsensitive",
                render=lambda _pdf, output, _size: output.write_bytes(
                    b"\x89PNG\r\n\x1a\nfixture"
                ),
                extract_text=lambda _path: "APPROVED PLAN",
                ocr_text=lambda _path: "",
                analyze_image=lambda _path, _size: drawing,
            )

        self.assertEqual(result["previews"], [])
        self.assertEqual(result["document_reviews"][0]["rejection_reason"], "private_or_sensitive")

    def test_one_document_failure_does_not_skip_later_plans(self):
        artifacts = [
            self.plan_artifact("siteplan.pdf"),
            self.plan_artifact("9th Floor Plan1.pdf"),
        ]
        drawing = {
            "dark_ratio": 0.08,
            "mid_tone_ratio": 0.05,
            "very_dark_ratio": 0.03,
            "edge_ratio": 0.08,
        }

        def download(url):
            if "siteplan" in url:
                raise RuntimeError("truncated document")
            return b"%PDF-1.4\nfloor-plan"

        with tempfile.TemporaryDirectory() as temp_dir:
            result = prepare_rera_plan_previews(
                artifacts,
                "PRM-PARTIAL",
                cache_root=Path(temp_dir),
                download=download,
                render=lambda _pdf, output, _size: output.write_bytes(
                    b"\x89PNG\r\n\x1a\nfixture"
                ),
                extract_text=lambda _path: "FLOOR PLAN",
                ocr_text=lambda _path: "",
                analyze_image=lambda _path, _size: drawing,
            )

        self.assertEqual(len(result["previews"]), 1)
        self.assertEqual(result["previews"][0]["buyer_label"], "Floor plan")
        self.assertEqual(result["document_reviews"][0]["status"], "failed")
        self.assertEqual(result["document_reviews"][1]["status"], "accepted")

    def test_listing_cache_preserves_exact_raw_receipt_bytes(self):
        body = b"""
        applicationNameList.push('ACK-1')
        applicationNameList2.push('PRM-1')
        applicationNameList3.push('Evidence Project')
        applicationNameList4.push('Evidence Builder')
        """

        class FakeSession:
            def get_bytes(self, url, timeout=60):
                assert url == "https://rera.karnataka.gov.in/viewAllProjects?language=en"
                assert timeout == 120
                return body

        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = Path(temp_dir)
            listing_cache = cache_dir / "listing.json"
            listing_raw = cache_dir / "listing.html"
            with patch("pipeline.skills.fetch_rera.LISTING_CACHE_PATH", listing_cache), patch(
                "pipeline.skills.fetch_rera.LISTING_RAW_CACHE_PATH", listing_raw
            ), patch("pipeline.skills.fetch_rera.ReraSession", FakeSession):
                entries = scrape_rera_listing(force=True)

            self.assertEqual(entries[0].registration_number, "PRM-1")
            self.assertEqual(listing_raw.read_bytes(), body)

    def test_rera_records_are_qualified_and_do_not_emit_sensitive_finance(self):
        detail = ReraProjectDetail(
            registration_number="PRM/KA/RERA/1251/446/PR/200811/003528",
            status="Approved",
            land_litigation=False,
            has_borrowing=True,
            has_mortgage=False,
            escrow_bank="Example Bank",
            escrow_account="1234567890",
            escrow_ifsc="EXAM0001234",
        )

        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertEqual(facts["rera_registered"].confidence, 0.95)
        self.assertEqual(
            facts["rera_registered"].answers_preferences,
            ["rera registration", "rera number"],
        )
        self.assertEqual(
            facts["rera_land_litigation"].display_template,
            "Promoter land-litigation declaration: {value}",
        )
        self.assertEqual(facts["rera_land_litigation"].confidence, 0.70)
        self.assertEqual(facts["rera_has_borrowing"].confidence, 0.70)
        self.assertNotIn("rera_escrow_account", facts)
        self.assertNotIn("rera_escrow_ifsc", facts)

        safety_terms = ("safe", "verified", "clear title", "financially safe")
        for fact in facts.values():
            text = " ".join([fact.display_template] + (fact.answers_preferences or [])).lower()
            self.assertFalse(any(term in text for term in safety_terms), text)

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

    def test_location_emits_rera_coordinate_audit_fact_only(self):
        detail = ReraProjectDetail(latitude="12.9698", longitude="77.75")

        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertEqual(
            facts["rera_lat_lng"].value,
            {"type": "Text", "data": "12.9698,77.75"},
        )
        self.assertNotIn("geo.latitude", facts)
        self.assertNotIn("geo.longitude", facts)

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

    def test_parse_brochure_documents_as_candidates_not_floor_plans(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu1" class="tab-pane">
          Project Type : Residential
          <p>Project Brochure 14-04-2022
            <a href="/download_jc?DOC_ID=abc">Godrej Splendour Brochure.pdf</a>
          </p>
          <p>3 GROUND FLOOR PLAN
            <a href="/download_jc?DOC_ID=def">3GROUND FLOOR PLAN.pdf</a>
          </p>
        </div>
        """
        search_result = ReraSearchResult(
            ack_number="ACK-1",
            registration_number="PRM-1",
            promoter_name="Godrej",
            project_name="Godrej Splendour",
            status="Registered",
            district="Bengaluru Urban",
            taluk="Bengaluru East",
            project_type="Residential",
            approved_on="",
            completion_date="",
            original_completion_date="",
            numeric_id="9197",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}
        artifacts = {artifact.document_kind: artifact for artifact in detail.document_artifacts}

        self.assertIn("brochure", artifacts)
        self.assertEqual(facts["brochure_asset_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["floor_plan_asset_count"].value, {"type": "Numeric", "data": 1})
        manifest = facts["rera_plan_artifact_manifest"].value["data"]
        self.assertIn('"kind":"brochure"', manifest)

    def test_parse_uploaded_document_manifest_from_rera_labels(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu2" class="tab-pane">
          <p>Approved Layout Plan <a href="/download_jc?DOC_ID=site">site.pdf</a></p>
          <p>Encumbrance Certificate <a href="/download_jc?DOC_ID=ec">ec.pdf</a></p>
          <p>BESCOM <a href="/download_jc?DOC_ID=bescom">bescom-noc.pdf</a></p>
          <p>Affidavit (Annexure - 49) <a href="/download_jc?DOC_ID=aff">annexure49.pdf</a></p>
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
            completion_date="",
            original_completion_date="",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}
        manifest = json.loads(facts["rera_document_manifest"].value["data"])
        by_kind = {item["kind"]: item for item in manifest}

        self.assertEqual(by_kind["site_plan"]["document_group"], "plans")
        self.assertEqual(by_kind["site_plan"]["preview_policy"], "auto_validate")
        self.assertEqual(by_kind["encumbrance_certificate"]["document_group"], "legal_land")
        self.assertEqual(by_kind["noc"]["source_field_label"], "BESCOM")
        self.assertEqual(by_kind["affidavit"]["document_group"], "affidavits")
        self.assertEqual(facts["rera_noc_document_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["rera_legal_land_document_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["rera_affidavit_document_count"].value, {"type": "Numeric", "data": 1})

    def test_skip_placeholder_uploaded_document_links(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu2" class="tab-pane">
          <p>Approved Layout Plan <a href="/download_jc?DOC_ID=">Floor Plan</a></p>
          <p>Approved Layout Plan <a href="/download_jc?DOC_ID=na">Not applicable.pdf</a></p>
          <p>Joint Development Agreement <a href="/download_jc?DOC_ID=missing">Not Available.pdf</a></p>
          <p>Approved Layout Plan <a href="/download_jc?DOC_ID=site">site.pdf</a></p>
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
            completion_date="",
            original_completion_date="",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}
        manifest = json.loads(facts["rera_document_manifest"].value["data"])

        self.assertEqual(len(manifest), 1)
        self.assertEqual(manifest[0]["kind"], "site_plan")
        self.assertEqual(facts["site_plan_asset_count"].value, {"type": "Numeric", "data": 1})
        self.assertNotIn("floor_plan_asset_count", facts)

    def test_parse_project_and_promoter_complaints_as_separate_scopes(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu-comp" class="tab-pane">
          Complaints on Promoter (2)
          <table>
            <tr><td>1</td><td>CMP/10/2024</td><td>12-01-2024</td><td>Refund after cancellation</td><td>DISPOSED</td></tr>
            <tr><td>2</td><td>CMP/11/2024</td><td>14-02-2024</td><td>Possession delay compensation</td><td>UNDER ENQUIRY</td></tr>
          </table>
        </div>
        <div id="menu-comp2" class="tab-pane">
          Complaints on Project (1)
          <table>
            <tr><td>1</td><td>CMP/20/2025</td><td>04-03-2025</td><td>Agreement payment dispute</td><td>POSTED FOR ORDERS</td></tr>
          </table>
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
            completion_date="",
            original_completion_date="",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}
        summary = json.loads(facts["rera_complaint_summary_manifest"].value["data"])
        by_scope = {item["scope"]: item for item in summary}

        self.assertEqual(detail.complaints_count, 1)
        self.assertEqual(facts["rera_complaints_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["rera_project_complaints_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["rera_promoter_complaints_count"].value, {"type": "Numeric", "data": 2})
        self.assertEqual(facts["rera_project_complaints_open_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["rera_promoter_complaints_disposed_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(by_scope["project"]["theme_counts"], {"agreement_payment": 1})
        self.assertEqual(
            by_scope["promoter"]["theme_counts"],
            {
                "cancellation": 1,
                "compensation": 1,
                "delay": 1,
                "possession": 1,
                "refund": 1,
            },
        )

    def test_parse_complaint_themes_keeps_fine_legal_and_facility_signals(self):
        html = """
        <div id="menu-comp2" class="tab-pane">
          Complaints on Project (5)
          <table>
            <tr><td>1</td><td>CMP/30/2025</td><td>04-03-2025</td><td>Khata and title document handover pending</td><td>UNDER ENQUIRY</td></tr>
            <tr><td>2</td><td>CMP/31/2025</td><td>05-03-2025</td><td>Land conversion approval and OC not provided</td><td>PENDING</td></tr>
            <tr><td>3</td><td>CMP/32/2025</td><td>06-03-2025</td><td>Parking allocation and clubhouse amenities dispute</td><td>DISPOSED</td></tr>
            <tr><td>4</td><td>CMP/33/2025</td><td>07-03-2025</td><td>Maintenance corpus demand letter with penal interest</td><td>POSTED FOR ORDERS</td></tr>
            <tr><td>5</td><td>CMP/34/2025</td><td>08-03-2025</td><td>False promise and misrepresentation by builder</td><td>UNDER ENQUIRY</td></tr>
          </table>
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
            completion_date="",
            original_completion_date="",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}
        summary = json.loads(facts["rera_complaint_summary_manifest"].value["data"])[0]

        self.assertEqual(
            summary["theme_counts"],
            {
                "agreement_payment": 1,
                "amenities": 1,
                "approval_oc_cc": 1,
                "builder_conduct": 1,
                "interest_demand": 1,
                "khata": 1,
                "maintenance": 1,
                "parking": 1,
                "registration_document": 1,
                "title_land": 2,
            },
        )

    def test_parse_waterford_cross_tab_schedule_fields(self):
        html = """
        <div id="home" class="tab-pane">Promoter Details</div>
        <div id="menu1" class="tab-pane">
          Project Start Date : 15-09-2020 Proposed Project Completion Date : 15-09-2024
          Total Area Of Land (Sq Mtr) : 66823
          Total Open Area (Sq Mtr) : 59380
          No of Parking for Sale : 106
        </div>
        <div id="menu3" class="tab-pane">
          Tower Details FAR Sanctioned : 1.88 Number of Towers : 7
          Tower Details - B1-T1 No. of Floors 22 Total No. of Units 129 Total No. of Parking 161
          Tower Details - B1-T2 No. of Floors 23 Total No. of Units 92 Total No. of Parking 137
          Tower Details - B5-T7 No. of Floors 24 Total No. of Units 92 Total No. of Parking 194
          Total No of Units 689
          Sewage Treatment Plant (STP) Yes
        </div>
        """
        search_result = ReraSearchResult(
            ack_number="ACK-1",
            registration_number="PRM/KA/RERA/1251/446/PR/200811/003528",
            promoter_name="Prestige",
            project_name="Prestige Waterford",
            status="Registered",
            district="Bengaluru Urban",
            taluk="Bengaluru East",
            project_type="Residential",
            approved_on="",
            completion_date="15-09-2024",
            original_completion_date="15-12-2023",
            numeric_id="6981",
        )

        detail = parse_rera_detail(html, search_result)
        facts = {fact.key: fact for fact in rera_detail_to_facts(detail)}

        self.assertEqual(facts["project_start_date"].value, {"type": "Text", "data": "2020-09-15"})
        self.assertEqual(facts["project_unit_count"].value, {"type": "Numeric", "data": 689})
        self.assertEqual(facts["project_tower_count"].value, {"type": "Numeric", "data": 7})
        self.assertEqual(facts["project_max_floor_count"].value, {"type": "Numeric", "data": 24})
        self.assertEqual(facts["parking_total_car_count"].value, {"type": "Numeric", "data": 492})
        self.assertEqual(facts["parking_offered_for_sale_count"].value, {"type": "Numeric", "data": 106})
        self.assertEqual(facts["stp_count"].value, {"type": "Numeric", "data": 1})
        self.assertEqual(facts["project_units_per_acre"].value, {"type": "Numeric", "data": 41.73})


if __name__ == "__main__":
    unittest.main()
