import type { ProjectionPoint } from "./model.ts";

export type GapLeader = "buy" | "rent";

export type WealthGapPoint = Pick<ProjectionPoint, "year" | "buyNetWorth" | "rentNetWorth">;

type ScaledGapPoint = WealthGapPoint & {
  x: number;
  buyY: number;
  rentY: number;
};

export type WealthGapArea = {
  leader: GapLeader;
  path: string;
};

type Scale = {
  x: (year: number) => number;
  y: (value: number) => number;
};

function pointLeader(point: WealthGapPoint): GapLeader | null {
  const delta = point.buyNetWorth - point.rentNetWorth;
  if (delta > 0) return "buy";
  if (delta < 0) return "rent";
  return null;
}

function scalePoint(point: WealthGapPoint, scale: Scale): ScaledGapPoint {
  return {
    ...point,
    x: scale.x(point.year),
    buyY: scale.y(point.buyNetWorth),
    rentY: scale.y(point.rentNetWorth),
  };
}

function interpolateGapPoint(
  start: WealthGapPoint,
  end: WealthGapPoint,
  ratio: number,
): WealthGapPoint {
  return {
    year: start.year + (end.year - start.year) * ratio,
    buyNetWorth: start.buyNetWorth + (end.buyNetWorth - start.buyNetWorth) * ratio,
    rentNetWorth: start.rentNetWorth + (end.rentNetWorth - start.rentNetWorth) * ratio,
  };
}

function areaPath(points: ScaledGapPoint[]): string {
  const buyEdge = points
    .map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(1)},${point.buyY.toFixed(1)}`);
  const rentEdge = [...points]
    .reverse()
    .map((point) => `L${point.x.toFixed(1)},${point.rentY.toFixed(1)}`);
  return [...buyEdge, ...rentEdge, "Z"].join(" ");
}

export function linePathForValues(
  values: readonly number[],
  x: (year: number) => number,
  y: (value: number) => number,
): string {
  return values
    .map((value, year) => `${year === 0 ? "M" : "L"}${x(year).toFixed(1)},${y(value).toFixed(1)}`)
    .join(" ");
}

export function buildWealthGapAreas(
  points: readonly WealthGapPoint[],
  scale: Scale,
): WealthGapArea[] {
  const runs: Array<{ leader: GapLeader; points: ScaledGapPoint[] }> = [];

  function addSegment(leader: GapLeader | null, start: WealthGapPoint, end: WealthGapPoint) {
    if (!leader) return;
    const scaledStart = scalePoint(start, scale);
    const scaledEnd = scalePoint(end, scale);
    const lastRun = runs.at(-1);
    if (lastRun?.leader === leader) {
      lastRun.points.push(scaledEnd);
      return;
    }
    runs.push({ leader, points: [scaledStart, scaledEnd] });
  }

  for (let index = 0; index < points.length - 1; index += 1) {
    const start = points[index];
    const end = points[index + 1];
    if (!start || !end) continue;

    const startDelta = start.buyNetWorth - start.rentNetWorth;
    const endDelta = end.buyNetWorth - end.rentNetWorth;
    const startLeader = pointLeader(start);
    const endLeader = pointLeader(end);

    if (startLeader && endLeader && startLeader !== endLeader) {
      const crossoverRatio = startDelta / (startDelta - endDelta);
      const crossover = interpolateGapPoint(start, end, crossoverRatio);
      addSegment(startLeader, start, crossover);
      addSegment(endLeader, crossover, end);
      continue;
    }

    addSegment(startLeader ?? endLeader, start, end);
  }

  return runs
    .filter((run) => run.points.length > 1)
    .map((run) => ({
      leader: run.leader,
      path: areaPath(run.points),
    }));
}
