import { getFixtureResponse } from "./dev-fixtures.ts";
import type { StoryMediaFrameInput } from "./propertyStory.ts";
import type { PropertyDetailResponse } from "./types.ts";

export type StoryLabCoverage = "rich" | "partial" | "sparse";
export type StoryLabImageCount = "none" | "single" | "many";
export type StoryLabLifecycle = "ready" | "under-construction";
export type StoryLabProvenance = "current" | "render" | "mixed";
export type StoryLabReviewState = "present" | "unresolved" | "empty";
export type StoryLabReraState = "complete" | "partial" | "missing";
export type StoryLabPropertyFixture =
  | "fixture-prestige-lakeside-3bhk"
  | "fixture-sobha-royal-pavilion-4bhk"
  | "fixture-vaswani-starlight-3bhk";

export type StoryLabFixtureOptions = {
  propertyId: StoryLabPropertyFixture;
  coverage: StoryLabCoverage;
  lifecycle: StoryLabLifecycle;
  reviews: StoryLabReviewState;
  rera: StoryLabReraState;
};

export type StoryLabMediaOptions = {
  count: StoryLabImageCount;
  provenance: StoryLabProvenance;
};

function cloneDetail(propertyId: StoryLabPropertyFixture): PropertyDetailResponse {
  const fixture = getFixtureResponse(
    `/api/properties/${propertyId}`,
  ) as PropertyDetailResponse | null;
  if (!fixture) throw new Error("Story Lab property fixture is unavailable");
  return JSON.parse(JSON.stringify(fixture)) as PropertyDetailResponse;
}

function addArrivalEvidence(
  detail: PropertyDetailResponse,
  frameCount: 1 | 4,
): void {
  const fixtureFrames = [
    {
      label: "Main road",
      distance_from_gate_m: 180,
      image_url: "/story-lab/arrival.webp",
      heading: 80,
    },
    {
      label: "Turn-in",
      distance_from_gate_m: 95,
      image_url: "/story-lab/property-hero.webp",
      heading: 120,
    },
    {
      label: "Final approach",
      distance_from_gate_m: 35,
      image_url: "/story-lab/inside.webp",
      heading: 160,
    },
    {
      label: "Gate",
      distance_from_gate_m: 0,
      image_url: "/story-lab/amenity.webp",
      heading: 190,
    },
  ].slice(0, frameCount).map((frame, index) => ({
    ...frame,
    pitch: 0,
    fov: 82,
    capture_date: "2026-06",
    source_url: `https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=12.945,77.69&heading=${frame.heading}&frame=${index + 1}`,
  }));
  const existingSections = detail.evidence?.sections
    .filter((section) => section.kind !== "approach_road") ?? [];
  detail.evidence = {
    property_id: detail.property.id,
    entity_refs: detail.evidence?.entity_refs ?? {
      property_entity_id: `property:${detail.property.id}`,
      society_entity_id: "society:story-lab",
      area_entity_id: `area:${detail.property.area.toLocaleLowerCase().replace(/\s+/g, "-")}`,
    },
    serving_bundle_version: detail.evidence?.serving_bundle_version,
    sections: [
      ...existingSections,
      {
        kind: "approach_road",
        title: "Approach road",
        summary: "",
        subtitle: "",
        priority: 2,
        source_types: ["Google Street View"],
        entity_ids: ["society:story-lab"],
        items: [],
        missing: [],
        media: [{
          kind: "street_view_strip",
          provider: "Google Street View",
          title: "Approach road",
          caption: "",
          capture_date_label: "Jun 2026",
          coverage_quality: "strong",
          frames: fixtureFrames,
        }],
      },
    ],
  };
}

export function storyLabDetailFixture(
  options: StoryLabFixtureOptions,
): PropertyDetailResponse {
  const detail = cloneDetail(options.propertyId);
  detail.property.hero_image = "";
  detail.property.images = [];
  detail.property.possession_status =
    options.lifecycle === "ready" ? "Ready to move" : "Under construction";
  detail.project_status =
    options.lifecycle === "ready" ? "ready_to_move" : "under_construction";
  detail.project_status_display =
    options.lifecycle === "ready" ? "Ready to move" : "Under construction";
  detail.home_state_display = detail.project_status_display;

  if (options.reviews === "present") {
    detail.external_reviews = {
      google_rating: 4.3,
      google_review_count: 1_842,
      google_reviews_url: "https://www.google.com/maps",
      reviews: [{
        id: "story-lab-review",
        source: "Google",
        author: "Resident",
        rating: 4,
        date_label: "Recent",
        text: "Landscaping and shared spaces are consistently appreciated.",
        tone: "positive",
      }],
    };
  } else if (options.reviews === "unresolved") {
    detail.external_reviews = {
      google_reviews_url: "https://www.google.com/maps",
    };
  } else {
    delete detail.external_reviews;
  }

  if (options.rera === "complete") {
    detail.rera_report_ref = {
      registration_ids: ["PRM/KA/RERA/1251/446/PR/170915/000123"],
      href: `/property/${options.propertyId}/rera`,
      availability: "available",
    };
    detail.decision_check_summary = {
      tileLabel: "RERA",
      tone: "positive",
      registrationNumberCompact: "PRM/KA/.../000123",
      primaryCount: 2,
      totalCount: 2,
      primaryLabels: [
        {
          key: "sanction_plan_available",
          label: "Sanction plan available",
          severity: "positive",
          scope: "project",
          visualId: "layout",
          valueText: "1",
          priority: 28,
          confidence: 1,
          groupId: "documents",
          placement: "audit",
        },
        {
          key: "site_plan_available",
          label: "Site plan available",
          severity: "positive",
          scope: "project",
          visualId: "layout",
          valueText: "1",
          priority: 26,
          confidence: 1,
          groupId: "documents",
          placement: "more",
        },
      ],
    };
  } else if (options.rera === "partial") {
    detail.rera_report_ref = {
      registration_ids: [],
      href: `/property/${options.propertyId}/rera`,
      availability: "partial",
    };
    detail.decision_check_summary = {
      tileLabel: "RERA",
      tone: "neutral",
      registrationNumberCompact: "PRM/KA/.../000123",
      primaryCount: 0,
      totalCount: 0,
    };
  } else {
    detail.rera_report_ref = {
      registration_ids: [],
      href: `/property/${options.propertyId}/rera`,
      availability: "unavailable",
    };
    delete detail.decision_check_summary;
  }

  if (options.coverage === "rich") {
    detail.map_context = {
      home: {
        entity_id: "society:story-lab",
        name: detail.society?.name ?? detail.property.title,
        area: detail.property.area,
        latitude: 12.945,
        longitude: 77.69,
      },
      places: [{
        layer: "schools",
        name: "Neighbourhood school",
        source_type: "fixture",
      }],
    };
    addArrivalEvidence(detail, 4);
  } else if (options.coverage === "partial") {
    delete detail.map_context;
    addArrivalEvidence(detail, 1);
  } else {
    delete detail.map_context;
    if (detail.evidence) {
      detail.evidence.sections = detail.evidence.sections
        .filter((section) => section.kind !== "approach_road");
    }
  }

  if (options.coverage === "sparse") {
    detail.similar_properties = [];
    delete detail.external_reviews;
    detail.rera_report_ref = {
      registration_ids: [],
      href: `/property/${options.propertyId}/rera`,
      availability: "unavailable",
    };
    delete detail.decision_check_summary;
  } else if (options.coverage === "partial") {
    detail.similar_properties = detail.similar_properties.slice(0, 1);
  }

  return detail;
}

function mediaLifecycle(
  provenance: StoryLabProvenance,
  index: number,
): "current" | "proposed" {
  if (provenance === "current") return "current";
  if (provenance === "render") return "proposed";
  return index < 2 ? "current" : "proposed";
}

export function storyLabMediaFixture(
  options: StoryLabMediaOptions,
): StoryMediaFrameInput[] {
  if (options.count === "none") return [];
  const frames: StoryMediaFrameInput[] = [
    {
      id: "story-lab-estate",
      url: "/story-lab/property-hero.webp",
      role: "hero",
      focalPoint: { x: 0.5, y: 0.5 },
    },
    {
      id: "story-lab-arrival",
      url: "/story-lab/arrival.webp",
      role: "exterior",
      focalPoint: { x: 0.32, y: 0.58 },
    },
    {
      id: "story-lab-inside",
      url: "/story-lab/inside.webp",
      role: "building",
      focalPoint: { x: 0.5, y: 0.46 },
    },
    {
      id: "story-lab-amenity",
      url: "/story-lab/amenity.webp",
      role: "amenity",
      focalPoint: { x: 0.5, y: 0.5 },
    },
  ];
  const selected = options.count === "single" ? frames.slice(0, 1) : frames;
  return selected.map((frame, index) => {
    const lifecycle = mediaLifecycle(options.provenance, index);
    return {
      ...frame,
      lifecycle,
      sourceType:
        lifecycle === "current" ? "Site photograph" : "Builder render",
      capturedAt: lifecycle === "current" ? "2026-06-12" : undefined,
      sourceUrl: lifecycle === "proposed"
        ? "https://www.prestigeconstructions.com/"
        : undefined,
    };
  });
}
