/** Portable, renderer-independent plan coordinates: metres on the X/Z floor plane. */
export type Vec2 = readonly [number, number];
export type RoomKind =
  | "entry"
  | "living"
  | "kitchen"
  | "utility"
  | "bedroom"
  | "bathroom"
  | "balcony"
  | "hall";
export interface Room {
  id: string;
  name: string;
  kind: RoomKind;
  polygon: readonly Vec2[];
  sourceDimensions?: string;
}
export interface Opening {
  start: number;
  end: number;
  kind: "door" | "slider" | "window";
  /** Only traversable openings have connectivity. Null means the exterior. */
  connects?: readonly [string, string | null];
  sillM?: number;
  headM?: number;
}
export interface Wall {
  id: string;
  a: Vec2;
  b: Vec2;
  heightM?: number;
  opening?: Opening;
}
export interface HomePlan {
  version: 1;
  id: string;
  name: string;
  unit: string;
  units: "metres";
  source: {
    label: string;
    href?: string;
    image?: string;
    page?: number;
    status: "traced" | "validated" | "synthetic";
  };
  rooms: readonly Room[];
  walls: readonly Wall[];
  entranceWallId: string;
  architecture: { ceilingM: number; wallM: number; heightsVerified: boolean };
}
export interface TourPolicy {
  walkMps: number;
  eyeM: number;
  clearanceM: number;
  gridM: number;
  maxGridNodes: number;
  turnRadiansPerSecond: number;
  settleSeconds: number;
  inspectSeconds: number;
  entranceSeconds: number;
  roomPriority: Record<RoomKind, number>;
}
export const DEFAULT_POLICY: Readonly<TourPolicy> = Object.freeze({
  walkMps: 0.8,
  eyeM: 1.6,
  clearanceM: 0.18,
  gridM: 0.13,
  maxGridNodes: 120000,
  turnRadiansPerSecond: Math.PI / 4,
  settleSeconds: 1.2,
  inspectSeconds: 10,
  entranceSeconds: 3,
  roomPriority: {
    entry: 0,
    living: 1,
    balcony: 2,
    kitchen: 3,
    utility: 4,
    hall: 5,
    bedroom: 6,
    bathroom: 7,
  },
});
export interface Dimension {
  a: Vec2;
  b: Vec2;
  metres: number;
  label: "Length" | "Width";
}
export interface TourStop {
  roomId: string;
  point: Vec2;
  heading: number;
  dimensions: readonly Dimension[];
}
export interface PreparedHome {
  plan: HomePlan;
  policy: TourPolicy;
  entrance: Vec2;
  entranceHeading: number;
  stops: readonly TourStop[];
  routes: readonly (readonly Vec2[])[];
  bounds: { min: Vec2; max: Vec2; center: Vec2; span: number };
}
export type TourPhase =
  | "entrance"
  | "walking"
  | "settling"
  | "inspecting"
  | "paused"
  | "exploring"
  | "complete";
export interface TourFrame {
  position: Vec2;
  heading: number;
  pitch: number;
  phase: TourPhase;
  roomId: string | null;
  stopIndex: number;
  progress: number;
  showDimensions: boolean;
}
