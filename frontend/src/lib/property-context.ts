import presentation from "../../../app/config/ui/property-context.json" with { type: "json" };
import { resolvedProofFocus } from "./proof-focus.ts";
import type {
  PropertyContext,
  ContextFact,
  ContextFeature,
} from "../generated/PropertyContext.ts";
import type {
  SurfaceSceneResponse,
  SceneFeature,
  SceneLayer,
  SceneReceipt,
  SceneGeometry,
  ProofFocus,
  ArrivalSceneExperience,
  MapLayerExperience,
  MapPresentation,
} from "./types.ts";

type LayerRule = {
  id: string;
  label: string;
  family: string;
  relationClass: string;
  renderKind: string;
  factKeys: string[];
  featureLabels?: Record<string, string>;
  featureProperties?: Record<string, string>;
  featureValueLabels?: Record<string, Record<string, string>>;
  emptyState?: string;
  enabledByDefault?: boolean;
  rank?: number;
  icon?: string;
  sort?: string;
  sortPriorityFactKeys?: string[];
  maxItems?: number;
  expandedMaxItems?: number;
  spreadMinDistanceKm?: number;
  showReviewMetrics?: boolean;
  includeNameMarkers?: string[];
  mapPresentation?: MapPresentation;
  experience?: MapLayerExperience;
  metricFactKeys?: { rating?: string; reviewCount?: string };
  tone: "positive" | "neutral" | "caution" | "risk";
  kind: string;
};
type SurfaceRule = {
  id: string;
  scene?: { experience?: ArrivalSceneExperience; layers: LayerRule[] };
};

function valueText(fact: ContextFact): string {
  const value = fact.value.data;
  return typeof value === "object" && value !== null && "explanation" in value
    ? value.explanation
    : Array.isArray(value)
      ? value.join(", ")
      : String(value ?? "");
}
function receipt(fact: ContextFact): SceneReceipt {
  return {
    id: fact.id,
    entityId: fact.entityId,
    factKey: fact.factKey,
    claim: valueText(fact),
    evidence: fact.evidence,
    sourceType: fact.sourceType,
    sourceUrl: fact.sourceUrl ?? undefined,
    learnedAt: fact.observedAt,
    confidence: fact.confidence,
  };
}
function metric(
  feature: ContextFeature,
  key: string | undefined,
): number | undefined {
  const value = feature.attributes.find((fact) => fact.factKey === key)?.value
    .data;
  return typeof value === "number" ? value : undefined;
}
function geometryPoints(geometry: SceneGeometry): [number, number][] {
  switch (geometry.type) {
    case "Point":
      return [geometry.coordinates];
    case "LineString":
      return geometry.coordinates;
    case "Polygon":
      return geometry.coordinates.flat();
    case "MultiPolygon":
      return geometry.coordinates.flat(2);
  }
}

/** Presentation-only projection. Eligibility and evidence come from the wire resource. */
export function projectPropertyContext(
  context: PropertyContext,
  surfaceId: string,
  token?: string,
): SurfaceSceneResponse | null {
  const surface = (presentation.surfaces as unknown as SurfaceRule[]).find(
    (surface) => surface.id === surfaceId,
  );
  if (!surface?.scene) return null;
  const proof = context.matchedProof ?? undefined;
  const requestedFocus = proof
    ? resolvedProofFocus(proof, token ?? "")
    : undefined;
  let focus: ProofFocus | undefined;
  const features: SceneFeature[] = [];
  const receipts = new Map<string, SceneReceipt>();
  const layers: SceneLayer[] = [];
  for (const [index, rule] of surface.scene.layers.entries()) {
    const candidates = context.features.filter((feature) =>
      rule.factKeys.includes(feature.fact.factKey),
    );
    // A resolved spatial derivation is an additive overlay with the same durable inputs.
    const derivation = proof?.derivationChain[0];
    const source = proof?.sourceObservations[0];
    if (
      proof &&
      derivation &&
      source &&
      proof.geometry &&
      requestedFocus?.surfaceId === surfaceId &&
      requestedFocus.layerId === rule.id
    ) {
      const geometry = proof.geometry as SceneGeometry;
      if (
        ["Point", "LineString", "Polygon", "MultiPolygon"].includes(
          geometry.type,
        )
      ) {
        candidates.push({
          fact: {
            id: derivation.derivation_id,
            entityId: proof.subjectEntityId,
            factKey: proof.factKey,
            value: proof.value ?? {
              type: "Text",
              data: proof.targetLabel ?? "",
            },
            evidence: {
              snapshot_identity: proof.snapshotIdentity,
              subject_entity_id: proof.subjectEntityId,
              evidence_id: { kind: "derivation", id: derivation.derivation_id },
            },
            sourceType: source.provider,
            sourceUrl: source.sourceUrl ?? null,
            observedAt: source.observedAt,
            confidence: derivation.confidence,
          },
          target: {
            entityId: proof.targetEntityId ?? proof.subjectEntityId,
            name: proof.targetLabel ?? "",
            geometry,
            point: geometry.type === "Point" ? geometry.coordinates : null,
            geometrySource: null,
            geometryEvidence: derivation.input_evidence,
          },
          attributes: [],
          distance: derivation,
        });
      }
    }
    const projected = candidates.flatMap((candidate) => {
      const geometry: SceneGeometry | undefined =
        candidate.target?.geometry ??
        (candidate.target?.point
          ? { type: "Point", coordinates: candidate.target.point }
          : undefined);
      if (!geometry) return [];
      const label = candidate.target?.name ?? valueText(candidate.fact);
      if (
        rule.includeNameMarkers?.length &&
        !rule.includeNameMarkers.some((marker) =>
          label.toLowerCase().includes(marker.toLowerCase()),
        )
      )
        return [];
      const properties = Object.fromEntries(
        Object.entries(rule.featureProperties ?? {}).flatMap(
          ([key, factKey]) => {
            const fact = candidate.attributes.find(
              (fact) => fact.factKey === factKey,
            );
            return fact ? [[key, valueText(fact)]] : [];
          },
        ),
      );
      const feature: SceneFeature = {
        id: `${surfaceId}:${rule.id}:${candidate.fact.id}`,
        entityId: candidate.target?.entityId,
        layerId: rule.id,
        kind: rule.kind,
        label,
        shortLabel: rule.featureLabels?.[candidate.fact.factKey],
        geometry,
        coordinateQuality: "exact",
        properties,
        confidence: candidate.fact.confidence,
        metrics: {
          distanceM:
            candidate.distance?.value != null &&
            candidate.distance.unit === "km"
              ? Math.round(candidate.distance.value * 1000)
              : undefined,
          rating:
            rule.showReviewMetrics === false
              ? undefined
              : metric(candidate, rule.metricFactKeys?.rating),
          reviewCount:
            rule.showReviewMetrics === false
              ? undefined
              : metric(candidate, rule.metricFactKeys?.reviewCount),
        },
        display: {
          tone: rule.tone,
          icon: rule.icon,
          priority: rule.rank ?? index + 1,
        },
        receiptIds: [candidate.fact.id],
      };
      return [{ feature, candidate }];
    });
    const priority = (key: string) => {
      const rank = rule.sortPriorityFactKeys?.indexOf(key) ?? -1;
      return rank < 0 ? (rule.sortPriorityFactKeys?.length ?? 0) : rank;
    };
    projected.sort(
      (a, b) =>
        priority(a.candidate.fact.factKey) -
          priority(b.candidate.fact.factKey) ||
        (rule.sort === "reviews"
          ? (b.feature.metrics?.reviewCount ?? 0) -
            (a.feature.metrics?.reviewCount ?? 0)
          : 0) ||
        (rule.sort === "distance" || rule.sort === "reviews"
          ? (a.feature.metrics?.distanceM ?? Infinity) -
            (b.feature.metrics?.distanceM ?? Infinity)
          : 0) ||
        a.feature.id.localeCompare(b.feature.id),
    );
    const unique = projected.filter(
      (item, index, all) =>
        !all
          .slice(0, index)
          .some(
            (previous) =>
              previous.feature.entityId === item.feature.entityId &&
              JSON.stringify(previous.feature.geometry) ===
                JSON.stringify(item.feature.geometry),
          ),
    );
    const selected = unique.slice(0, rule.maxItems ?? unique.length);
    const matched = projected.find(
      ({ candidate }) =>
        proof?.factKey === candidate.fact.factKey &&
        (!proof.targetEntityId ||
          candidate.target?.entityId === proof.targetEntityId) &&
        (candidate.fact.evidence.evidence_id.kind === "derivation"
          ? proof.derivationChain.some(
              (item) =>
                item.derivation_id === candidate.fact.evidence.evidence_id.id,
            )
          : proof.sourceObservations.some(
              (source) =>
                candidate.fact.evidence.evidence_id.id === source.observationId,
            )),
    );
    if (matched && !selected.includes(matched)) selected.push(matched);
    if (
      matched &&
      requestedFocus?.surfaceId === surfaceId &&
      requestedFocus.layerId === rule.id
    ) {
      focus = {
        ...requestedFocus,
        featureId: matched.feature.id,
        receiptId: matched.candidate.fact.id,
        distanceM: matched.feature.metrics?.distanceM,
      };
    }
    for (const item of selected) {
      features.push(item.feature);
      receipts.set(item.candidate.fact.id, receipt(item.candidate.fact));
    }
    layers.push({
      id: rule.id,
      label: rule.label,
      family: rule.family,
      relationClass: rule.relationClass,
      renderKind: rule.renderKind,
      mapPresentation: rule.mapPresentation,
      experience: rule.experience,
      emptyState: rule.emptyState,
      featureValueLabels: rule.featureValueLabels,
      enabledByDefault: rule.enabledByDefault ?? true,
      rank: rule.rank ?? index + 1,
      availableCount: unique.length,
      shownCount: selected.length,
      fillState:
        selected.length === 0
          ? "empty"
          : selected.length < unique.length
            ? "partial"
            : "filled",
    });
  }
  const point = context.anchor.point ?? undefined;
  const geometry = context.anchor.geometry ?? undefined;
  const geometrySource = context.anchor.geometrySource;
  const points = features.flatMap((feature) =>
    geometryPoints(feature.geometry),
  );
  if (point) points.push(point);
  const filledLayers = layers.filter(
    (layer) => layer.fillState === "filled",
  ).length;
  const partialLayers = layers.filter(
    (layer) => layer.fillState === "partial",
  ).length;
  return {
    contractVersion: 1,
    snapshotIdentity: context.snapshotIdentity,
    propertyId: context.propertyId,
    entityRefs: context.entityRefs,
    surfaceId,
    anchor: {
      entityId: context.anchor.entityId,
      label: context.anchor.name,
      geometry: point ? { type: "Point", coordinates: point } : undefined,
      boundary:
        geometry && geometrySource
          ? {
              geometry,
              sourceType: geometrySource.sourceType,
              sourceUrl: geometrySource.sourceUrl ?? undefined,
              confidence: geometrySource.confidence,
            }
          : undefined,
      coordinateQuality: point ? "exact" : "missing",
    },
    experience: surface.scene.experience,
    proofFocus: focus,
    proofFocusStatus: focus ? "applied" : "notRequested",
    viewport: {
      center: point,
      bounds: points.length
        ? {
            west: Math.min(...points.map((p) => p[0])),
            east: Math.max(...points.map((p) => p[0])),
            south: Math.min(...points.map((p) => p[1])),
            north: Math.max(...points.map((p) => p[1])),
          }
        : undefined,
    },
    layers,
    features,
    receipts: [...receipts.values()],
    relations: [],
    callouts: [],
    fillRate: {
      filledLayers,
      partialLayers,
      emptyLayers: layers.length - filledLayers - partialLayers,
      shownFeatures: features.length,
      availableFeatures: layers.reduce((n, l) => n + l.availableCount, 0),
      value: layers.length ? (filledLayers + partialLayers) / layers.length : 0,
    },
    gaps: layers
      .filter((layer) => layer.fillState !== "filled")
      .map((layer) => ({ layerId: layer.id, fillState: layer.fillState })),
  };
}
