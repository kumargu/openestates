import type {
  AreaTrackerMarket,
  AreaTrackerMetricDefinition,
  AreaTrackerMetricValue,
} from "./types.ts";

export function areaTrackerMetric(
  market: AreaTrackerMarket,
  definition: AreaTrackerMetricDefinition,
): AreaTrackerMetricValue | null {
  return market.metrics?.find((metric) => metric.id === definition.id) ?? null;
}
