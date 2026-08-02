import { normalizePlanInputs, type PlanInputs } from "./model.ts";

const PLAN_DRAFT_STORAGE_PREFIX = "openestates:buy-vs-rent-draft:";

export type PropertyPlanDraft = {
  version: 1;
  propertyId: string;
  inputs: PlanInputs;
  extraEmisPerYear: number;
  updatedAt: number;
};

export function planDraftStorageKey(propertyId: string): string {
  return `${PLAN_DRAFT_STORAGE_PREFIX}${encodeURIComponent(propertyId)}`;
}

function normalizeDraft(value: unknown, propertyId: string): PropertyPlanDraft | null {
  if (typeof value !== "object" || value == null) return null;
  const candidate = value as Partial<PropertyPlanDraft>;
  if (candidate.version !== 1 || candidate.propertyId !== propertyId || candidate.inputs == null) return null;
  if (!Number.isFinite(candidate.extraEmisPerYear) || !Number.isFinite(candidate.updatedAt)) {
    return null;
  }

  try {
    return {
      version: 1,
      propertyId,
      inputs: normalizePlanInputs(candidate.inputs),
      extraEmisPerYear: Math.max(0, Math.floor(candidate.extraEmisPerYear ?? 0)),
      updatedAt: candidate.updatedAt ?? 0,
    };
  } catch {
    return null;
  }
}

export function readPlanDraft(propertyId: string): PropertyPlanDraft | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(planDraftStorageKey(propertyId));
    return raw ? normalizeDraft(JSON.parse(raw), propertyId) : null;
  } catch {
    return null;
  }
}

export function writePlanDraft(
  propertyId: string,
  inputs: PlanInputs,
  extraEmisPerYear: number,
): PropertyPlanDraft {
  const draft: PropertyPlanDraft = {
    version: 1,
    propertyId,
    inputs: normalizePlanInputs(inputs),
    extraEmisPerYear: Math.max(0, Math.floor(extraEmisPerYear)),
    updatedAt: Date.now(),
  };
  window.localStorage.setItem(planDraftStorageKey(propertyId), JSON.stringify(draft));
  return draft;
}
