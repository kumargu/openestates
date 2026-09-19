import type { HomePlan, Room, Vec2, Wall } from "../core/types.ts";
/** Synthetic topology fixtures, also available in the demo. All plan-specific work lives here. */
export function configuration(bedrooms: 0 | 1 | 2 | 4, rotate = 0): HomePlan {
  const rooms: Room[] = [
    {
      id: "living",
      name: bedrooms ? "Living room" : "Studio",
      kind: "living",
      polygon: [
        [0, 0],
        [5, 0],
        [5, 4],
        [0, 4],
      ],
    },
  ];
  const walls: Wall[] = [
    {
      id: "entry",
      a: [0, 0],
      b: [5, 0],
      opening: {
        kind: "door",
        start: 0.38,
        end: 0.62,
        connects: ["living", null],
      },
    },
    { id: "west", a: [0, 0], b: [0, 4] },
    { id: "east", a: [5, 0], b: [5, 4] },
  ];
  let parent = "living",
    z = 4;
  for (let i = 0; i < bedrooms; i++) {
    const id = `bed-${i + 1}`;
    rooms.push({
      id,
      name: `Bedroom ${i + 1}`,
      kind: "bedroom",
      polygon: [
        [0, z],
        [5, z],
        [5, z + 3],
        [0, z + 3],
      ],
    });
    walls.push(
      {
        id: `door-${i}`,
        a: [0, z],
        b: [5, z],
        opening: {
          kind: "door",
          start: 0.38,
          end: 0.62,
          connects: [parent, id],
        },
      },
      { id: `west-${i}`, a: [0, z], b: [0, z + 3] },
      { id: `east-${i}`, a: [5, z], b: [5, z + 3] },
    );
    parent = id;
    z += 3;
  }
  walls.push({ id: "south", a: [0, z], b: [5, z] });
  const turn = ([x, y]: Vec2): Vec2 => [
    x * Math.cos(rotate) - y * Math.sin(rotate),
    x * Math.sin(rotate) + y * Math.cos(rotate),
  ];
  return {
    version: 1,
    id: `synthetic-${bedrooms}`,
    name: bedrooms ? `${bedrooms}-bedroom test home` : "Compact studio",
    unit: "Synthetic geometry",
    units: "metres",
    source: { label: "Generated validation fixture", status: "synthetic" },
    entranceWallId: "entry",
    architecture: { ceilingM: 2.8, wallM: 0.15, heightsVerified: false },
    rooms: rooms.map((r) => ({ ...r, polygon: r.polygon.map(turn) })),
    walls: walls.map((w) => ({ ...w, a: turn(w.a), b: turn(w.b) })),
  };
}
