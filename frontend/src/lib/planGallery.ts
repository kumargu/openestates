import type { ProjectPlansView } from "./types.ts";

export type PlanGalleryItem = {
  id: string;
  kind: string;
  label: string;
  detail?: string;
  previewUrl: string;
  thumbnailUrl: string;
};

function usablePreviewUrl(value?: string): string | null {
  if (!value) return null;
  if (value.startsWith("/media/")) return value;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) ? url.toString() : null;
  } catch {
    return null;
  }
}

export function planGalleryItems(plans?: ProjectPlansView): PlanGalleryItem[] {
  if (!plans) return [];
  const items: PlanGalleryItem[] = [];
  const siteUrl = usablePreviewUrl(plans.site_overview?.preview_url);
  if (plans.site_overview && siteUrl) {
    items.push({
      id: plans.site_overview.artifact_id,
      kind: "site_plan",
      label: plans.site_overview.label,
      previewUrl: siteUrl,
      thumbnailUrl: usablePreviewUrl(plans.site_overview.thumbnail_url) ?? siteUrl,
    });
  }
  for (const plan of plans.floor_plans ?? []) {
    const previewUrl = usablePreviewUrl(plan.preview_url);
    if (!previewUrl) continue;
    const detail = [
      plan.carpet_area_sqft
        ? `${plan.carpet_area_sqft.toLocaleString("en-IN")} sq ft carpet`
        : null,
      plan.sale_area_sqft
        ? `${plan.sale_area_sqft.toLocaleString("en-IN")} sq ft sale area`
        : null,
    ].filter(Boolean).join(" · ");
    items.push({
      id: plan.artifact_id,
      kind: "floor_plan",
      label: plan.title,
      detail: detail || undefined,
      previewUrl,
      thumbnailUrl: usablePreviewUrl(plan.thumbnail_url) ?? previewUrl,
    });
  }
  for (const plan of plans.filed_plan_previews ?? []) {
    const previewUrl = usablePreviewUrl(plan.preview_url);
    if (!previewUrl) continue;
    items.push({
      id: plan.artifact_id,
      kind: plan.kind,
      label: plan.label,
      previewUrl,
      thumbnailUrl: usablePreviewUrl(plan.thumbnail_url) ?? previewUrl,
    });
  }
  return items;
}

export function hasPlanGalleryItems(plans?: ProjectPlansView): boolean {
  return planGalleryItems(plans).length > 0;
}
