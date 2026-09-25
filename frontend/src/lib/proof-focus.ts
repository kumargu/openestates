import contextPresentation from "../../../app/config/ui/property-context.json" with { type: "json" };
import type { ProofFocus, SearchProofResolution, SearchResultItem, SurfaceSceneResponse } from "./types.ts";

export function proofFocusRendered(
  focus: ProofFocus | undefined,
  scene: SurfaceSceneResponse | null,
): boolean {
  if (!focus) return false;
  if (focus.destinationKind !== "scene") return true;
  return scene?.proofFocusStatus === "applied"
    && scene.proofFocus?.proofToken === focus.proofToken;
}

function proofDestination(factKey: string) {
  const surfaces: ReadonlyArray<{ id: string; proofHandoff?: { kind: string; targetId: string; factKeys?: string[] }; scene?: { layers: Array<{ id: string; factKeys: string[] }> } }> = contextPresentation.surfaces;
  for (const surface of surfaces) {
    if (!("proofHandoff" in surface) || !surface.proofHandoff) continue;
    const handoff = surface.proofHandoff;
    const layer = surface.scene?.layers.find((layer) => layer.factKeys.some((key) => key === factKey));
    if (layer || handoff.factKeys?.includes(factKey)) return {
      surfaceId: surface.id, layerId: layer?.id, kind: handoff.kind, targetId: handoff.targetId,
    };
  }
  return undefined;
}

export function resolvedProofFocus(proof: SearchProofResolution, token: string): ProofFocus | undefined {
  const destination = proofDestination(proof.factKey);
  if (!destination) return undefined;
  const value = proof.value?.data;
  const display = typeof value === "string" || typeof value === "number" || typeof value === "boolean"
    ? String(value) : Array.isArray(value) ? value.join(", ") : undefined;
  return {
    proofToken: token,
    surfaceId: destination.surfaceId,
    layerId: destination.layerId ?? "",
    destinationKind: destination.kind,
    targetId: destination.targetId,
    factKey: proof.factKey,
    entityId: proof.targetEntityId,
    matchedLabel: proof.targetLabel,
    matchedValue: display && proof.unit ? `${display} ${proof.unit}` : display,
    sourceUrl: proof.sourceObservations.find((source) => source.sourceUrl)?.sourceUrl,
    reason: "Matched your search",
  };
}

export type PropertyProofMatch = {
  value: string;
  sourceUrl?: string;
};

export function propertyProofMatch(
  focus: ProofFocus | undefined,
  targetId: string,
  sourceUrl?: string,
): PropertyProofMatch | undefined {
  if (focus?.targetId !== targetId) return undefined;
  const value = focus.matchedValue?.trim() || focus.matchedLabel?.trim();
  if (!value) return undefined;
  return { value, sourceUrl };
}

const DEFAULT_PROPERTY_SURFACE_ID = "around_this_home";

/** Card priority is backend-owned; display text never selects evidence. */
export function primaryProofFocus(
  result: Pick<SearchResultItem, "reasons">,
): ProofFocus | undefined {
  const reason = result.reasons?.find((reason) => reason.showOnCard) ?? result.reasons?.[0];
  return reason ? {
    surfaceId: "", layerId: "", factKey: "",
    reason: reason.explanation, proofToken: reason.proofToken,
  } : undefined;
}

export function initialPropertySurfaceId(focus?: ProofFocus): string {
  if (focus?.destinationKind && focus.destinationKind !== "scene") {
    return DEFAULT_PROPERTY_SURFACE_ID;
  }
  return focus?.surfaceId.trim() || DEFAULT_PROPERTY_SURFACE_ID;
}

export function propertySceneProofFocus(focus?: ProofFocus): ProofFocus | undefined {
  return focus?.destinationKind === "section" ? undefined : focus;
}

export function identityReceiptLabel(proof: SearchProofResolution): string | undefined {
  const labels: Record<string, string> = contextPresentation.receiptRelationLabels;
  const template = labels[proof.relation];
  return template && proof.targetLabel ? template.replace("{target}", proof.targetLabel) : undefined;
}
