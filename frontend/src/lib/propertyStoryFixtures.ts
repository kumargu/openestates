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
  } else if (options.rera === "partial") {
    detail.rera_report_ref = {
      registration_ids: [],
      href: `/property/${options.propertyId}/rera`,
      availability: "partial",
    };
  } else {
    detail.rera_report_ref = {
      registration_ids: [],
      href: `/property/${options.propertyId}/rera`,
      availability: "unavailable",
    };
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
  } else {
    delete detail.map_context;
  }

  if (options.coverage === "sparse") {
    detail.similar_properties = [];
    delete detail.external_reviews;
    detail.rera_report_ref = {
      registration_ids: [],
      href: `/property/${options.propertyId}/rera`,
      availability: "unavailable",
    };
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
