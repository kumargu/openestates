import { normalizePlanInputs, type PlanInputs } from "./model.ts";

const PLAN_DRAFT_STORAGE_PREFIX = "openestates:buy-vs-rent-draft:";
/**
 * Version 1 autosaved the baseline on load, so a stored draft could not be told
 * apart from a default. Version 2 stores buyer edits only; older drafts are
 * dropped so improved defaults reach returning buyers.
 */
const PLAN_DRAFT_VERSION = 2;

export type PropertyPlanDraft = {
  version: typeof PLAN_DRAFT_VERSION;
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
  if (
    candidate.version !== PLAN_DRAFT_VERSION
    || candidate.propertyId !== propertyId
    || candidate.inputs == null
  ) {
    return null;
  }
  if (!Number.isFinite(candidate.extraEmisPerYear) || !Number.isFinite(candidate.updatedAt)) {
    return null;
  }

  try {
    return {
      version: PLAN_DRAFT_VERSION,
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
    version: PLAN_DRAFT_VERSION,
    propertyId,
    inputs: normalizePlanInputs(inputs),
    extraEmisPerYear: Math.max(0, Math.floor(extraEmisPerYear)),
    updatedAt: Date.now(),
  };
  window.localStorage.setItem(planDraftStorageKey(propertyId), JSON.stringify(draft));
  return draft;
}

/** Reset drops the buyer's edits so the plan reopens on current defaults. */
export function clearPlanDraft(propertyId: string): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.removeItem(planDraftStorageKey(propertyId));
  } catch {
    // A full or blocked store is not worth failing a reset over.
  }
}

export type PlanStatus = "loading" | "ready" | "no_price" | "not_found" | "error";

export function canPersistPlanDraft(
  routePropertyId: string | undefined,
  loadedPropertyId: string | undefined,
  status: PlanStatus,
): boolean {
  return status === "ready"
    && Boolean(routePropertyId)
    && routePropertyId === loadedPropertyId;
}
