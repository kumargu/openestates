export type ChartInsets = Readonly<{
  top: number;
  right: number;
  bottom: number;
  left: number;
}>;

export type ChartPoint = Readonly<{ x: number; y: number }>;

export type LinearScale = Readonly<{
  map: (value: number) => number;
  invert: (value: number) => number;
}>;

export function linearScale(
  domain: readonly [number, number],
  range: readonly [number, number],
): LinearScale {
  const domainSpan = domain[1] - domain[0];
  const rangeSpan = range[1] - range[0];
  const safeDomainSpan = domainSpan === 0 ? 1 : domainSpan;
  const safeRangeSpan = rangeSpan === 0 ? 1 : rangeSpan;
  return {
    map: (value) => range[0] + ((value - domain[0]) / safeDomainSpan) * rangeSpan,
    invert: (value) => domain[0] + ((value - range[0]) / safeRangeSpan) * domainSpan,
  };
}

export function extent(values: readonly number[], includeZero = false): [number, number] {
  const finite = values.filter(Number.isFinite);
  const minimum = finite.length > 0 ? Math.min(...finite) : 0;
  const maximum = finite.length > 0 ? Math.max(...finite) : 1;
  const lower = includeZero ? Math.min(0, minimum) : minimum;
  const upper = includeZero ? Math.max(0, maximum) : maximum;
  if (lower === upper) return [lower, lower + 1];
  return [lower, upper];
}

export function paddedExtent(
  values: readonly number[],
  paddingRatio = 0.08,
  includeZero = false,
): [number, number] {
  const [minimum, maximum] = extent(values, includeZero);
  const padding = (maximum - minimum) * paddingRatio;
  return [includeZero && minimum === 0 ? 0 : minimum - padding, maximum + padding];
}

export function linePath(points: readonly ChartPoint[]): string {
  return points
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ");
}

/**
 * A monotone visual interpolation that passes through every computed point.
 * Control points remain inside each segment, so the curve cannot overshoot
 * financial values or invent a dip between observations.
 */
export function smoothLinePath(points: readonly ChartPoint[]): string {
  if (points.length === 0) return "";
  if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;

  const slopes = points.slice(1).map((point, index) => {
    const previous = points[index];
    return (point.y - previous.y) / Math.max(Number.EPSILON, point.x - previous.x);
  });
  const tangents = points.map((_, index) => {
    if (index === 0) return slopes[0];
    if (index === points.length - 1) return slopes.at(-1)!;
    const before = slopes[index - 1];
    const after = slopes[index];
    if (before === 0 || after === 0 || Math.sign(before) !== Math.sign(after)) return 0;
    return 2 / (1 / before + 1 / after);
  });

  let path = `M ${points[0].x} ${points[0].y}`;
  for (let index = 0; index < points.length - 1; index += 1) {
    const start = points[index];
    const end = points[index + 1];
    const width = end.x - start.x;
    const slope = slopes[index];
    let startTangent = tangents[index];
    let endTangent = tangents[index + 1];
    if (slope === 0) {
      startTangent = 0;
      endTangent = 0;
    } else {
      const startRatio = startTangent / slope;
      const endRatio = endTangent / slope;
      const magnitude = Math.hypot(startRatio, endRatio);
      if (magnitude > 3) {
        const scale = 3 / magnitude;
        startTangent = scale * startRatio * slope;
        endTangent = scale * endRatio * slope;
      }
    }
    path += ` C ${start.x + width / 3} ${start.y + startTangent * width / 3}, ${end.x - width / 3} ${end.y - endTangent * width / 3}, ${end.x} ${end.y}`;
  }
  return path;
}

export function areaPath(
  points: readonly ChartPoint[],
  baseline: number,
): string {
  if (points.length === 0) return "";
  return `${linePath(points)} L ${points.at(-1)!.x} ${baseline} L ${points[0].x} ${baseline} Z`;
}

export function bandPath(
  upper: readonly ChartPoint[],
  lower: readonly ChartPoint[],
): string {
  if (upper.length === 0 || lower.length === 0) return "";
  const reverseLower = [...lower].reverse();
  return `${linePath(upper)} ${reverseLower
    .map((point) => `L ${point.x} ${point.y}`)
    .join(" ")} Z`;
}

export function nearestIndex(
  clientX: number,
  bounds: Pick<DOMRect, "left" | "width">,
  svgWidth: number,
  insets: ChartInsets,
  pointCount: number,
): number {
  if (pointCount <= 1) return 0;
  const svgX = ((clientX - bounds.left) / Math.max(1, bounds.width)) * svgWidth;
  const plotWidth = svgWidth - insets.left - insets.right;
  const ratio = (svgX - insets.left) / Math.max(1, plotWidth);
  return Math.max(0, Math.min(pointCount - 1, Math.round(ratio * (pointCount - 1))));
}

export function chartTickIndexes(length: number, maximumTicks = 6): number[] {
  if (length <= 1) return [0];
  const last = length - 1;
  const step = Math.max(1, Math.ceil(last / Math.max(1, maximumTicks - 1)));
  const ticks: number[] = [];
  for (let index = 0; index < last; index += step) ticks.push(index);
  ticks.push(last);
  return [...new Set(ticks)];
}

export function stackedSegments(values: readonly number[]): Array<Readonly<{
  value: number;
  start: number;
  end: number;
}>> {
  let total = 0;
  return values.map((value) => {
    const safeValue = Math.max(0, value);
    const segment = { value: safeValue, start: total, end: total + safeValue };
    total += safeValue;
    return segment;
  });
}
