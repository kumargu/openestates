import assert from "node:assert/strict";
import test from "node:test";
import {
  httpUrl,
  kindLabel,
  reportSections,
  safeLabels,
  visibleDocumentSections,
} from "../src/lib/reraReportView.ts";
import type { ReraDossier, ReraDocumentSection } from "../src/lib/types.ts";

const BASE_DOSSIER: ReraDossier = {
  property_id: "property:test",
  society_id: "society:test",
  summary_cards: [],
  compare_items: [],
  complaint_sections: [],
  document_sections: [],
  timeline: {},
  legal_checks: [],
  source: {
    registered: true,
    registration_number: "PRM/KA/RERA/1251/446/PR/TEST",
    last_verified: "2026-07-30",
  },
};

test("RERA report falls back to generic compare facts without inventing empty rows", () => {
  const sections = reportSections({
    ...BASE_DOSSIER,
    compare_items: [
      { key: "project_complaints", label: "Project complaints", value: "3", tone: "watch", labels: ["legal"] },
      { key: "open_complaints", label: "Open complaints", value: "unknown", tone: "neutral", labels: [] },
      { key: "noc_documents", label: "NOC documents", value: "6", tone: "neutral", labels: [] },
    ],
  });

  assert.equal(sections.length, 1);
  assert.equal(sections[0].title, "Facts");
  assert.deepEqual(sections[0].facts.map((fact) => fact.label), [
    "Project complaints",
    "NOC documents",
  ]);
  assert.equal(sections[0].facts[1].learned_at, "2026-07-30");
});

test("configured RERA fact sections are preserved as the primary report shape", () => {
  const sections = reportSections({
    ...BASE_DOSSIER,
    fact_sections: [{
      id: "builder",
      title: "Builder",
      facts: [{
        key: "builder_average_delay_months",
        label: "Builder average delay",
        value: "8 months",
        tone: "caution",
        labels: ["risk"],
        confidence: 0.8,
        learned_at: "2026-07-30",
      }],
    }],
    compare_items: [
      { key: "project_complaints", label: "Project complaints", value: "3", tone: "watch", labels: ["legal"] },
    ],
  });

  assert.deepEqual(sections.map((section) => section.id), ["builder"]);
  assert.equal(sections[0].facts[0].value, "8 months");
});

test("RERA document sections hide empty groups and invalid links", () => {
  const sections: ReraDocumentSection[] = [
    {
      group: "noc",
      label: "NOC documents",
      count: 2,
      kinds: ["noc"],
      preview_available_count: 0,
      hidden_count: 0,
      items: [
        {
          artifact_id: "one",
          label: "Fire NOC",
          kind: "fire_noc",
          source_url: "https://rera.karnataka.gov.in/download_jc?DOC_ID=abc",
        },
        {
          artifact_id: "two",
          label: "Invalid",
          kind: "noc",
          source_url: "javascript:alert(1)",
        },
      ],
    },
    {
      group: "empty",
      label: "Empty",
      count: 1,
      kinds: ["other"],
      preview_available_count: 0,
      hidden_count: 1,
      items: [{
        artifact_id: "three",
        label: "Missing",
        kind: "other",
        source_url: "",
      }],
    },
  ];

  const visible = visibleDocumentSections(sections);

  assert.equal(visible.length, 1);
  assert.equal(visible[0].group, "noc");
  assert.deepEqual(visible[0].items.map((item) => item.label), ["Fire NOC"]);
});

test("RERA report helpers keep labels readable and notebook tags bounded", () => {
  assert.equal(httpUrl("ftp://example.com/file"), null);
  assert.equal(kindLabel("latest_encumbrance_certificate"), "Latest Encumbrance Certificate");
  assert.deepEqual(safeLabels(["risk", "risk", "legal", "builder", "delay"], "builder_delay"), [
    "risk",
    "legal",
    "builder",
    "delay",
  ]);
  assert.deepEqual(safeLabels([], "land_litigation"), ["risk", "legal"]);
});
