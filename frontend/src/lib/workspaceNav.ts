import type { NavigationMode } from "./navigationContext.ts";

export type WorkspaceView = "browse" | "home" | "notebook" | "compare" | "rera" | "plan";

export type WorkspaceNavItem = {
  view: WorkspaceView;
  label: string;
  icon: "browse" | "listing" | "notebook" | "compare" | "rera";
  to: string;
  active: boolean;
  /** False when the item needs a focused shortlist home and none is set. */
  available: boolean;
};

const MAX_WORKSPACE_COMPARE_HOMES = 4;

export function workspaceComparedIds(value: string | null | undefined): string[] {
  if (!value) return [];
  return [...new Set(value.split(",").map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_WORKSPACE_COMPARE_HOMES);
}

export function activeWorkspaceView(pathname: string): WorkspaceView {
  if (pathname === "/workspace" || pathname === "/notebook") return "notebook";
  if (pathname === "/workspace/compare" || pathname === "/compare") return "compare";
  if (/^\/workspace\/buy-vs-rent(?:\/[^/]+)?$/.test(pathname)) return "plan";
  if (/^\/property\/[^/]+\/rera$/.test(pathname)) return "rera";
  if (/^\/property\/[^/]+\/plan$/.test(pathname)) return "plan";
  if (/^\/property\/[^/]+$/.test(pathname)) return "home";
  return "browse";
}

export function workspaceNavItems(
  focusedId: string,
  activeView: WorkspaceView,
  options: {
    mode?: "discovery" | "property-context" | "workspace";
    compareIds?: string[];
  } = {},
): WorkspaceNavItem[] {
  const encodedId = focusedId ? encodeURIComponent(focusedId) : "";
  const hasFocus = Boolean(encodedId);
  const detailHref = hasFocus ? `/property/${encodedId}` : "/";
  const reraHref = hasFocus ? `/property/${encodedId}/rera` : "/";
  const compareIds = options.compareIds ?? [];
  const compareHref = workspaceCompareHref(compareIds, focusedId);

  const mode = options.mode ?? "workspace";
  if (mode === "property-context") {
    return [
      {
        view: "browse",
        label: "Explore",
        icon: "browse",
        to: "/",
        active: false,
        available: true,
      },
      {
        view: "home",
        label: "Property overview",
        icon: "listing",
        to: detailHref,
        active: activeView === "home",
        available: hasFocus,
      },
      { view: "rera", label: "RERA evidence", icon: "rera", to: reraHref, active: activeView === "rera", available: hasFocus },
      { view: "notebook", label: "Workspace", icon: "notebook", to: "/workspace", active: false, available: true },
    ];
  }

  return [
    { view: "browse" as const, label: "Explore", icon: "browse" as const, to: "/", available: true },
    { view: "notebook" as const, label: "Workspace", icon: "notebook" as const, to: "/workspace", available: true },
    { view: "compare" as const, label: compareIds.length >= 2 ? `Compare ${compareIds.length}` : "Compare", icon: "compare" as const, to: compareHref, available: compareIds.length >= 2 },
    { view: "rera" as const, label: "RERA evidence", icon: "rera" as const, to: reraHref, available: hasFocus },
  ].map((item) => ({
    ...item,
    active: item.view === "notebook"
      ? ["notebook", "plan"].includes(activeView)
      : item.view === activeView,
  }));
}

export function shouldShowWorkspaceSidebar(
  mode: NavigationMode,
  savedHomeCount: number,
): boolean {
  if (mode === "property-context" || mode === "workspace") return true;
  return savedHomeCount > 0;
}

export function workspaceCompareHref(ids: string[], focusId?: string): string {
  const uniqueIds = workspaceComparedIds(ids.join(","));
  if (uniqueIds.length < 2) return "/workspace/compare";
  const params = new URLSearchParams();
  params.set("ids", uniqueIds.join(","));
  if (focusId && uniqueIds.includes(focusId)) params.set("focus", focusId);
  return `/workspace/compare?${params.toString()}`;
}

export function workspaceBuyVsRentHref(
  propertyId?: string | null,
  compareIds: string[] = [],
): string {
  const path = propertyId
    ? `/workspace/buy-vs-rent/${encodeURIComponent(propertyId)}`
    : "/workspace/buy-vs-rent";
  const ids = workspaceComparedIds(compareIds.join(","));
  if (ids.length < 2) return path;
  const params = new URLSearchParams();
  params.set("from", "compare");
  params.set("ids", ids.join(","));
  if (propertyId && ids.includes(propertyId)) params.set("focus", propertyId);
  return `${path}?${params.toString()}`;
}

export function workspaceFocusedHomeId(
  requestedId: string | null | undefined,
  storedId: string | null | undefined,
  availableIds: string[],
): string {
  const validIds = [...new Set(availableIds.map((id) => id.trim()).filter(Boolean))];
  if (requestedId && validIds.includes(requestedId)) return requestedId;
  if (storedId && validIds.includes(storedId)) return storedId;
  return validIds[0] ?? "";
}

export function workspacePlanReplacementId(
  requestedId: string | undefined,
  availableIds: string[],
): string | null {
  const validIds = [...new Set(availableIds.map((id) => id.trim()).filter(Boolean))];
  if (requestedId && validIds.includes(requestedId)) return null;
  return validIds[0] ?? null;
}
