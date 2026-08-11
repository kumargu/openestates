from __future__ import annotations

import json
import unittest

from pipeline.skills.rera_regulatory_intelligence import (
    DocumentScope,
    EventDraft,
    RelationshipDraft,
    build_regulatory_source_records,
    privacy_is_valid,
    redact_public_text,
    redacted_document,
    resolve_current_effects,
    structured_list_event_candidates,
)


REGISTRATION = "PRM/KA/RERA/1251/446/PR/200811/003528"


def event(
    event_id: str,
    *,
    event_class: str = "final_finding",
    event_type: str = "authority_finding",
    current_effect: str = "The Authority recorded a project-specific finding.",
    stage: str = "final_authority_order",
    assertion_mode: str = "authority_order",
    source_trust: str = "primary_authority",
    page: int | None = 3,
    quote: str | None = "The Authority hereby records the finding.",
    disposition: str | None = "allowed",
) -> EventDraft:
    return EventDraft(
        event_id=event_id,
        event_class=event_class,
        event_type=event_type,
        occurred_at="2026-01-15",
        issuer="K-RERA",
        proceeding_ref=f"CMP/{event_id}/2026",
        decision_stage=stage,
        disposition=disposition,
        current_effect=current_effect,
        affected_scope="registered project",
        assertion_mode=assertion_mode,
        source_trust=source_trust,
        page=page,
        supporting_quote=quote,
        extraction_confidence=0.91,
    )


def records(
    events: list[EventDraft],
    *,
    verified: list[EventDraft] | None = None,
    document_format: str = "pdf",
    scope: DocumentScope | None = None,
    relationships: list[RelationshipDraft] | None = None,
) -> list[dict]:
    document = redacted_document(
        source_url="https://rera.karnataka.gov.in/orders/fixture.pdf",
        issuer="K-RERA",
        pages=["Public order text without personal data."],
        document_format=document_format,
    )
    return build_regulatory_source_records(
        registration_number=REGISTRATION,
        receipt_id="rera_receipt:sha256:fixture",
        capture_id="rera_capture:sha256:fixture",
        observed_at="2026-08-11T06:00:00+00:00",
        document=document,
        scope=scope or DocumentScope(registration_number=REGISTRATION),
        extracted_events=events,
        verified_events=events if verified is None else verified,
        extracted_relationships=relationships or [],
        verified_relationships=relationships or [],
    )


def payloads(rows: list[dict], kind: str = "regulatory_event") -> list[dict]:
    return [json.loads(row["raw_value"]) for row in rows if row["kind"] == kind]


class ReraRegulatoryIntelligenceTests(unittest.TestCase):
    def test_structured_list_accepts_only_exact_registration_rows(self):
        html = """
            <table id="projectList">
              <tr><th>PROJECT NO</th><th>ORDER CATEGORY</th><th>K-RERA ORDER DATE</th><th>K-RERA ORDER COPY</th></tr>
              <tr><td>PRM/KA/RERA/1251/446/PR/200811/003528</td><td>PROJECT EXTENSION</td><td>22-02-2023</td><td><a href="/download_jc?DOC_ID=exact">PDF</a></td></tr>
              <tr><td>PRM/KA/RERA/1251/446/PR/999999/999999</td><td>PROJECT REVOKED</td><td>23-02-2023</td><td><a href="/download_jc?DOC_ID=other">PDF</a></td></tr>
            </table>
        """
        candidates = structured_list_event_candidates(
            registration_number=REGISTRATION,
            issuer="K-RERA",
            base_url="https://rera.karnataka.gov.in",
            list_config={
                "id": "project_orders",
                "exact_registration_table": {
                    "table_id": "projectList",
                    "registration_header": "PROJECT NO",
                    "event_type_header": "ORDER CATEGORY",
                    "occurred_at_header": "K-RERA ORDER DATE",
                    "document_header": "K-RERA ORDER COPY",
                    "date_format": "%d-%m-%Y",
                    "decision_stage": "registry_order",
                    "assertion_mode": "authority_order",
                    "event_mappings": [
                        {
                            "source_value": "PROJECT EXTENSION",
                            "event_class": "registration_status",
                            "event_type": "registration_extended",
                            "current_effect": "K-RERA recorded a project extension.",
                        }
                    ],
                },
            },
            html=html,
        )
        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0].event.event_type, "registration_extended")
        self.assertEqual(candidates[0].event.assertion_mode, "authority_order")
        self.assertEqual(candidates[0].event.occurred_at, "2023-02-22")
        self.assertEqual(
            candidates[0].document_url,
            "https://rera.karnataka.gov.in/download_jc?DOC_ID=exact",
        )

    def test_redaction_removes_public_pii_before_provider_input(self):
        redacted = redact_public_text(
            "Buyer Asha Rao, 9876543210, asha@example.com, PAN ABCDE1234F\n"
            "Address: 12 Sample Road\nSignature by Asha Rao",
            natural_person_names=["Asha Rao"],
        )
        self.assertTrue(privacy_is_valid(redacted, ["Asha Rao"]))
        self.assertNotIn("9876543210", redacted)
        self.assertNotIn("ABCDE1234F", redacted)

    def test_structured_official_status_promotes_without_models(self):
        status = event(
            "lapsed",
            event_class="registration_restriction",
            event_type="registration_lapsed",
            current_effect="K-RERA lists the registration as lapsed.",
            stage="registry_status",
            assertion_mode="registry_record",
            page=None,
            quote=None,
            disposition=None,
        )
        row = payloads(records([status], verified=[], document_format="structured_list"))[0]
        self.assertFalse(row["promotion"]["unresolved_contradiction"])
        self.assertEqual(row["promotion"]["document_format"], "structured_list")

    def test_brigade_laguna_negative_declaration_is_not_title_clearance(self):
        disclosure = event(
            "brigade-laguna-litigation",
            event_class="promoter_disclosure",
            event_type="litigation_declaration",
            current_effect="Promoter declared no pending land litigation in this filing.",
            stage="promoter_affidavit",
            assertion_mode="promoter_declaration",
            source_trust="promoter_filing",
            disposition=None,
        )
        row = payloads(records([disclosure]))[0]
        self.assertEqual(row["assertion_mode"], "promoter_declaration")
        self.assertNotIn("clear title", row["current_effect"].lower())

    def test_embassy_greenshore_keeps_mortgage_proceedings_and_difference(self):
        greenshore = [
            event(
                "greenshore-mortgage",
                event_class="promoter_disclosure",
                event_type="mortgage_disclosure",
                current_effect="Promoter disclosed an HDFC mortgage.",
                stage="promoter_affidavit",
                assertion_mode="promoter_declaration",
                source_trust="promoter_filing",
            ),
            event(
                "greenshore-proceedings",
                event_class="promoter_disclosure",
                event_type="litigation_disclosure",
                current_effect="Promoter disclosed seven pending land proceedings.",
                stage="promoter_affidavit",
                assertion_mode="promoter_declaration",
                source_trust="promoter_filing",
            ),
            event(
                "greenshore-difference",
                event_class="filed_discrepancy",
                event_type="mortgage_records_differ",
                current_effect="Filed mortgage records differ.",
                stage="openestates_discrepancy",
                assertion_mode="registry_record",
            ),
        ]
        effects = [row["current_effect"] for row in payloads(records(greenshore))]
        self.assertEqual(len(effects), 3)
        self.assertIn("Promoter disclosed an HDFC mortgage.", effects)
        self.assertIn("Promoter disclosed seven pending land proceedings.", effects)
        self.assertIn("Filed mortgage records differ.", effects)

    def test_section_3_denials_remain_declarations_not_adverse_findings(self):
        denials = [
            event(
                f"section-3-denial-{index}",
                event_class="promoter_disclosure",
                event_type="section_3_declaration",
                current_effect="Promoter declared that Section 3(1) was not violated.",
                stage="promoter_affidavit",
                assertion_mode="promoter_declaration",
                source_trust="promoter_filing",
                disposition=None,
            )
            for index in (1, 2)
        ]
        rows = payloads(records(denials))
        self.assertTrue(all(row["assertion_mode"] == "promoter_declaration" for row in rows))
        self.assertTrue(all(row["event_class"] == "promoter_disclosure" for row in rows))

    def test_krsna_laburnum_order_keeps_each_official_remedy(self):
        types = {
            "unauthorized_floors",
            "registration_revoked",
            "account_freeze",
            "forensic_audit",
        }
        krsna = [
            event(
                event_type,
                event_class="enforcement" if event_type != "unauthorized_floors" else "final_finding",
                event_type=event_type,
                current_effect=f"The Authority recorded {event_type.replace('_', ' ')}.",
            )
            for event_type in types
        ]
        self.assertEqual({row["event_type"] for row in payloads(records(krsna))}, types)

    def test_fixture_matrix_promotes_complete_chronology(self):
        matrix = [
            event("interim", event_class="interim_order", stage="interim_order"),
            event("dismissed", event_class="historical", stage="final_authority_order", disposition="dismissed"),
            event("ao-allowed", stage="final_ao_order", disposition="allowed"),
            event("authority-modified", stage="final_authority_order", disposition="modified"),
            event("recovery-issued", event_class="enforcement", event_type="recovery_issued"),
            event("recovery-recovered", event_class="historical", event_type="recovery_recovered"),
            event("lapsed", event_class="registration_restriction", event_type="registration_lapsed"),
            event("extension", event_class="final_finding", event_type="registration_extended"),
        ]
        self.assertEqual(len(payloads(records(matrix))), len(matrix))

    def test_appeal_does_not_imply_stay_but_explicit_stay_changes_effect(self):
        order = event("order", current_effect="The Authority order remains active.")
        appeal = event("appeal", event_class="pending_proceeding", event_type="appeal_filed")
        appeal_relation = RelationshipDraft(
            relationship_id="appeals-order",
            relationship_type="appeals",
            from_event_id="appeal",
            to_event_id="order",
            occurred_at="2026-02-01",
            issuer="K-RERA",
        )
        self.assertEqual(
            {item.event_id: item.current_effect for item in resolve_current_effects([order, appeal], [appeal_relation])}["order"],
            "The Authority order remains active.",
        )
        stay = appeal_relation.model_copy(
            update={
                "relationship_id": "stays-order",
                "relationship_type": "stays",
                "effect_text": "The Authority stayed this order on 1 February 2026.",
            }
        )
        self.assertEqual(
            {item.event_id: item.current_effect for item in resolve_current_effects([order, appeal], [stay])}["order"],
            "The Authority stayed this order on 1 February 2026.",
        )

    def test_ambiguous_unregistered_project_match_is_quarantined(self):
        unregistered = event(
            "unregistered",
            event_class="registration_restriction",
            event_type="unregistered_under_investigation",
            current_effect="K-RERA lists this project as unregistered and under investigation.",
            stage="investigation_list",
        )
        ambiguous_scope = DocumentScope(project_match=True, promoter_match=True, location_match=False)
        row = payloads(records([unregistered], scope=ambiguous_scope))[0]
        self.assertEqual(row["promotion"]["scope_resolution"], "unresolved")

    def test_output_is_deterministic_and_contains_exact_pdf_support(self):
        finding = event("supported")
        first = records([finding])
        second = records([finding])
        self.assertEqual(first, second)
        row = payloads(first)[0]
        self.assertEqual(row["page"], 3)
        self.assertEqual(row["supporting_quote"], "The Authority hereby records the finding.")


if __name__ == "__main__":
    unittest.main()
