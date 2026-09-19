import type { HomePlan, Room, RoomKind, Vec2, Wall } from "../core/types.ts";

// Source adapter only. The engine never sees brochure pixels or apartment-specific camera poses.
// PWF_05.pdf p14: approximate digitisation, calibrated using printed bedroom spans.
const point = (x: number, y: number): Vec2 => [(x - 620) / 63, (y - 570) / 63];
const poly = (p: number[][]): Vec2[] => p.map(([x, y]) => point(x, y));
const rect = (x: number, y: number, w: number, h: number) =>
  poly([
    [x, y],
    [x + w, y],
    [x + w, y + h],
    [x, y + h],
  ]);
const room = (
  id: string,
  name: string,
  kind: RoomKind,
  polygon: Vec2[],
  sourceDimensions: string,
): Room => ({ id, name, kind, polygon, sourceDimensions });
const rooms: Room[] = [
  room(
    "foyer",
    "Entrance foyer",
    "entry",
    rect(836, 478, 105, 133),
    "5′4″ × 6′7″",
  ),
  room(
    "living",
    "Living & dining",
    "living",
    poly([
      [393, 478],
      [836, 478],
      [836, 777],
      [721, 777],
      [721, 714],
      [393, 714],
    ]),
    "23′0″ × 12′0″",
  ),
  room(
    "kitchen",
    "Kitchen",
    "kitchen",
    poly([
      [651, 405],
      [759, 405],
      [759, 221],
      [940, 221],
      [940, 478],
      [651, 478],
    ]),
    "9′0″ × 13′3″",
  ),
  room("utility", "Utility", "utility", rect(759, 132, 181, 89), "9′0″ × 4′6″"),
  room(
    "bed2",
    "Bedroom 02",
    "bedroom",
    poly([
      [300, 255],
      [545, 255],
      [545, 405],
      [651, 405],
      [651, 478],
      [300, 478],
    ]),
    "12′6″ × 11′0″",
  ),
  room(
    "bed1",
    "Bedroom 01",
    "bedroom",
    rect(393, 714, 223, 272),
    "11′8″ × 13′8″",
  ),
  room(
    "bed3",
    "Bedroom 03",
    "bedroom",
    rect(721, 777, 219, 236),
    "11′0″ × 12′0″",
  ),
  room(
    "balcony",
    "Living balcony",
    "balcony",
    rect(300, 478, 93, 236),
    "5′0″ × 12′0″",
  ),
  room(
    "balcony2",
    "Bedroom balcony",
    "balcony",
    rect(300, 775, 93, 211),
    "4′6″ × 11′0″",
  ),
  room(
    "bath1",
    "Attached bathroom · 01",
    "bathroom",
    rect(616, 780, 105, 180),
    "5′0″ × 9′0″",
  ),
  room(
    "bath2",
    "Attached bathroom · 02",
    "bathroom",
    rect(545, 229, 106, 176),
    "5′0″ × 8′8″",
  ),
  room(
    "bath3",
    "Attached bathroom · 03",
    "bathroom",
    rect(836, 611, 104, 166),
    "5′0″ × 8′0″",
  ),
  room(
    "powder",
    "Powder room",
    "bathroom",
    rect(651, 287, 108, 118),
    "5′0″ × 5′9″",
  ),
];
const walls: Wall[] = [];
function wall(
  id: string,
  a: number[],
  b: number[],
  opening?: Wall["opening"],
  heightM?: number,
) {
  walls.push({
    id,
    a: point(a[0], a[1]),
    b: point(b[0], b[1]),
    opening,
    heightM,
  });
}
const door = (
  start: number,
  end: number,
  a: string,
  b: string | null,
  kind: "door" | "slider" = "door",
): Wall["opening"] => ({ start, end, kind, connects: [a, b] });
const window = (start: number, end: number): Wall["opening"] => ({
  start,
  end,
  kind: "window",
  sillM: 0.95,
  headM: 2.4,
});
wall("bed2-n", [300, 255], [545, 255], window(0.59, 0.82));
wall("bed2-w", [300, 255], [300, 478], window(0.28, 0.72));
wall("bath2-n", [545, 229], [651, 229], window(0.1, 0.5));
wall("bath2-w", [545, 229], [545, 405]);
wall("bath2-e", [651, 229], [651, 405]);
wall("bath2-door", [545, 405], [651, 405], door(0.12, 0.55, "bed2", "bath2"));
wall("powder-n", [651, 287], [759, 287], window(0.17, 0.62));
wall("powder-e", [759, 287], [759, 405]);
wall(
  "powder-door",
  [651, 405],
  [759, 405],
  door(0.08, 0.55, "kitchen", "powder"),
);
wall("utility-n", [759, 132], [940, 132]);
wall("utility-w", [759, 132], [759, 221], window(0.1, 0.9));
// Open utility threshold has zero wall segments; its connectivity is explicit.
wall(
  "utility-opening",
  [759, 221],
  [940, 221],
  door(0, 1, "kitchen", "utility"),
);
wall("kitchen-w", [759, 221], [759, 287]);
wall("kitchen-east", [940, 132], [940, 511]);
wall("bed2-door", [300, 478], [651, 478], door(0.77, 0.93, "living", "bed2"));
wall(
  "kitchen-door",
  [651, 478],
  [940, 478],
  door(0, 0.35, "living", "kitchen"),
);
wall("foyer-opening", [836, 478], [836, 611], door(0, 1, "foyer", "living"));
wall("main-entry", [940, 511], [940, 579], door(0, 1, "foyer", null));
wall("east", [940, 579], [940, 1013], window(0.21, 0.33));
wall("bath3-n", [836, 611], [940, 611]);
wall("bath3-w", [836, 611], [836, 777]);
wall("bath3-door", [836, 777], [940, 777], door(0.25, 0.71, "bed3", "bath3"));
wall("bed3-door", [721, 777], [836, 777], door(0.02, 0.51, "living", "bed3"));
wall("bed3-w", [721, 777], [721, 1013]);
wall("bed3-s", [721, 1013], [940, 1013], window(0.22, 0.74));
wall("bed1-door", [393, 714], [624, 714], door(0.7, 1, "living", "bed1"));
wall("bath1-n", [616, 780], [721, 780]);
wall("bath1-door", [616, 780], [616, 960], door(0.03, 0.29, "bed1", "bath1"));
wall("bath1-s", [616, 960], [721, 960], window(0.13, 0.68));
wall("bed1-s", [393, 986], [616, 986]);
wall("bed1-e", [616, 960], [616, 1013]);
wall(
  "balcony2-door",
  [393, 714],
  [393, 986],
  door(0.25, 0.78, "bed1", "balcony2", "slider"),
);
wall("balcony-n", [300, 478], [393, 478]);
wall("balcony-rail", [300, 478], [300, 714], undefined, 1.05);
wall("balcony-s", [300, 714], [393, 714]);
wall(
  "balcony-door",
  [393, 478],
  [393, 714],
  door(0.17, 0.83, "living", "balcony", "slider"),
);
wall("balcony2-n", [300, 775], [393, 775]);
wall("balcony2-rail", [300, 775], [300, 986], undefined, 1.05);
wall("balcony2-s", [300, 986], [393, 986]);

export const waterford: HomePlan = {
  version: 1,
  id: "waterford-b1",
  name: "Prestige Waterford",
  unit: "Type B1 · 3 bedrooms",
  units: "metres",
  source: {
    label: "Prestige Waterford brochure · page 14",
    page: 14,
    status: "traced",
  },
  rooms,
  walls,
  entranceWallId: "main-entry",
  architecture: { ceilingM: 2.85, wallM: 0.16, heightsVerified: false },
};
