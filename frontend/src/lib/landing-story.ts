export const LANDING_STORY_SCENE_IDS = [
  "resolve",
  "reveal",
  "remember",
  "record",
  "converge",
] as const;

export type LandingStorySceneId = typeof LANDING_STORY_SCENE_IDS[number];
export type LandingStoryPresentation = "wide" | "tile";

export type LandingStoryImage = {
  src: string;
  srcNarrow: string;
  width: number;
  height: number;
};

export type LandingStoryChapter = {
  id: LandingStorySceneId;
  side: "left" | "right";
  presentation: LandingStoryPresentation;
  title: string;
  description: string;
  imageAlt?: string;
  image?: LandingStoryImage;
};

function tileImage(
  name: string,
  width: number,
  height: number,
): LandingStoryImage {
  return {
    src: `/landing/tiles/${name}.webp`,
    srcNarrow: `/landing/tiles/${name}-960.webp`,
    width,
    height,
  };
}

export const LANDING_STORY_CHAPTERS: LandingStoryChapter[] = [
  {
    id: "resolve",
    side: "right",
    presentation: "wide",
    title: "Start with the life you want",
    description: "Describe the life. We return a few homes, each with why it matched.",
  },
  {
    id: "reveal",
    side: "left",
    presentation: "tile",
    title: "Open a home, not a listing",
    description: "Map, commute and project checks stay on the same home.",
    imageAlt: "A home opened with nearby map context, commute, reviews and project checks attached.",
    image: tileImage("03-map-evidence", 1600, 901),
  },
  {
    id: "remember",
    side: "right",
    presentation: "tile",
    title: "Keep your judgment",
    description: "Notes and a visit list stay attached, so the reasoning does not vanish.",
    imageAlt: "A buying notebook with a saved home, visit checklist and linked evidence.",
    image: tileImage("05-buyer-notebook", 1600, 1024),
  },
  {
    id: "record",
    side: "right",
    presentation: "wide",
    title: "Read the official record",
    description: "Registration, progress and approvals — the receipts, not a brochure.",
    imageAlt: "A RERA record showing registration, promoter, approvals and construction progress.",
    image: tileImage("04-rera-evidence", 1600, 963),
  },
  {
    id: "converge",
    side: "left",
    presentation: "wide",
    title: "Make the tradeoffs visible",
    description: "Compare two homes, then see the buy versus rent horizon.",
    imageAlt: "Two homes compared on commute, water, RERA and reviews, with a buy versus rent chart.",
    image: tileImage("06-tradeoffs", 1600, 878),
  },
];

export const LANDING_RESOLVE_QUERY = "3BHK under 2Cr with strong reviews and generous open space";
