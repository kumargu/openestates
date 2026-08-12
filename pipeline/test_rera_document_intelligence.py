import unittest

from pipeline.skills.rera_document_intelligence import (
    canonical_rera_society_entity_id,
    classify_rera_document,
    load_document_policy,
    select_rera_document_previews,
)


def artifact(
    artifact_id: str,
    label: str,
    role: str,
    *,
    source_url: str = "https://rera.test/document",
) -> dict:
    classification = classify_rera_document(label)
    return {
        "artifact_id": artifact_id,
        "kind": classification["kind"],
        "label": label,
        "source_url": source_url,
        "preview_role": role,
        "preview_policy": "content_review_required",
    }


def rendered(*artifact_ids: str) -> dict:
    return {
        artifact_id: {
            "preview_url": f"/media/{artifact_id}.png",
            "source_hash": f"hash-{artifact_id}",
            "page": 1,
        }
        for artifact_id in artifact_ids
    }


class ReraDocumentIntelligenceTest(unittest.TestCase):
    def test_society_identity_comes_only_from_normalized_registration_number(self):
        self.assertEqual(
            canonical_rera_society_entity_id(
                "  prm/ka/rera/1251/446/pr/300924/007105  "
            ),
            "society:rera-688242e8e3711955",
        )

    def test_policy_is_valid_and_has_no_project_specific_rules(self):
        policy = load_document_policy()
        serialized = str(policy).lower()

        self.assertEqual(policy["version"], 2)
        self.assertNotIn("godrej", serialized)
        self.assertNotIn("sobha", serialized)
        self.assertNotIn("prestige", serialized)

    def test_explicit_filename_role_wins_over_portal_field_context(self):
        photo = classify_rera_document(
            "Project Photos.pdf", "Floor Plan", "/download_jc?DOC_ID=photo"
        )
        license_doc = classify_rera_document(
            "Modified Building License.pdf", "Floor Plan", "/download_jc?DOC_ID=license"
        )
        contract = classify_rera_document(
            "Draft AFS.pdf", "Brochure of Current Project", "/download_jc?DOC_ID=afs"
        )

        self.assertEqual(photo["preview_role"], "project_media")
        self.assertEqual(photo["kind"], "project_media")
        self.assertEqual(photo["classification_basis"], "label")
        self.assertEqual(license_doc["kind"], "building_license")
        self.assertEqual(license_doc["preview_policy"], "list_only")
        self.assertEqual(contract["kind"], "customer_contract_template")

    def test_filename_matching_handles_portal_separators(self):
        sensitive = classify_rera_document("GPL_PAN Card-signed.pdf")
        unavailable = classify_rera_document("Not_Available.jpg", "Section plan")

        self.assertEqual(sensitive["preview_role"], "promoter_financial")
        self.assertEqual(sensitive["preview_policy"], "hidden")
        self.assertEqual(unavailable["preview_role"], "placeholder")
        self.assertEqual(unavailable["preview_policy"], "hidden")

    def test_generic_filename_uses_source_field_context(self):
        classification = classify_rera_document(
            "site.pdf", "Approved Layout Plan", "/download_jc?DOC_ID=site"
        )

        self.assertEqual(classification["kind"], "site_plan")
        self.assertEqual(classification["classification_basis"], "source_field_label")

    def test_plural_approved_plan_page_is_a_preview_candidate(self):
        classification = classify_rera_document("Approved Plans-pages-2.pdf")

        self.assertEqual(classification["kind"], "sanction_plan")
        self.assertEqual(classification["preview_policy"], "content_review_required")

    def test_selection_requires_render_and_rejects_not_applicable_content(self):
        artifacts = [
            artifact("master", "Updated Master Plan.pdf", "master_plan"),
            artifact("site", "Approved Site Plan.pdf", "site_plan", source_url="https://rera.test/site"),
            artifact("photo", "Project Photos.pdf", "project_media", source_url="https://rera.test/photo"),
            artifact("missing", "3 BHK Unit Plan.pdf", "unit_plan", source_url="https://rera.test/unit"),
        ]

        result = select_rera_document_previews(
            artifacts,
            rendered("master", "site", "photo"),
            {"site": "Approved layout plan is NOT APPLICABLE for this project."},
        )

        self.assertEqual(
            [item["artifact_id"] for item in result["selected"]],
            ["master"],
        )
        reasons = {item["artifact_id"]: item["reason"] for item in result["excluded"]}
        self.assertEqual(reasons["site"], "content_excluded")
        self.assertEqual(reasons["photo"], "role_not_selected")
        self.assertEqual(reasons["missing"], "missing_rendered_preview")

    def test_selection_applies_role_caps_and_overview_deduplication(self):
        artifacts = [
            artifact("master", "Master Plan.pdf", "master_plan"),
            artifact("site", "Site Plan.pdf", "site_plan", source_url="https://rera.test/site"),
            artifact("photo-a", "Project Photo A.pdf", "project_media", source_url="https://rera.test/a"),
            artifact("photo-b", "Project Photo B.pdf", "project_media", source_url="https://rera.test/b"),
            artifact("photo-c", "Project Photo C.pdf", "project_media", source_url="https://rera.test/c"),
        ]

        result = select_rera_document_previews(
            artifacts,
            rendered("master", "site", "photo-a", "photo-b", "photo-c"),
        )

        self.assertEqual(
            [item["artifact_id"] for item in result["selected"]],
            ["master"],
        )
        reasons = {item["artifact_id"]: item["reason"] for item in result["excluded"]}
        self.assertEqual(reasons["site"], "dedupe_bucket")
        self.assertEqual(reasons["photo-a"], "role_not_selected")
        self.assertEqual(reasons["photo-b"], "role_not_selected")
        self.assertEqual(reasons["photo-c"], "role_not_selected")

    def test_selection_deduplicates_identical_content_across_filenames(self):
        artifacts = [
            artifact("elevation", "Sectional Elevation.pdf", "elevation"),
            artifact(
                "sanction",
                "Approved Building Plan.pdf",
                "sanction_plan",
                source_url="https://rera.test/sanction",
            ),
        ]
        rendered_previews = rendered("elevation", "sanction")
        rendered_previews["sanction"]["source_hash"] = rendered_previews["elevation"][
            "source_hash"
        ]

        result = select_rera_document_previews(artifacts, rendered_previews)

        self.assertEqual(
            [item["artifact_id"] for item in result["selected"]],
            ["elevation"],
        )
        self.assertIn(
            {"artifact_id": "sanction", "reason": "duplicate_content"},
            result["excluded"],
        )

    def test_sparse_plan_set_is_not_filled_with_a_brochure(self):
        artifacts = [
            artifact("sanction", "Sanction Plan.pdf", "sanction_plan"),
            artifact("brochure", "Brochure.pdf", "brochure", source_url="https://rera.test/brochure"),
        ]

        result = select_rera_document_previews(
            artifacts,
            rendered("sanction", "brochure"),
        )

        self.assertEqual(
            [item["artifact_id"] for item in result["selected"]],
            ["sanction"],
        )
        self.assertIn(
            {"artifact_id": "brochure", "reason": "role_not_selected"},
            result["excluded"],
        )


if __name__ == "__main__":
    unittest.main()
