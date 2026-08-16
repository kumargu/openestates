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
    discoveryHref?: string;
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
        to: options.discoveryHref ?? "/",
        active: false,
        available: true,
      },
      {
        view: "home",
        label: "Home",
        icon: "listing",
        to: detailHref,
        active: activeView === "home",
        available: hasFocus,
      },
      { view: "rera", label: "RERA", icon: "rera", to: reraHref, active: activeView === "rera", available: hasFocus },
      { view: "notebook", label: "Notes", icon: "notebook", to: "/workspace", active: false, available: true },
    ];
  }

  return [
    { view: "browse" as const, label: "Explore", icon: "browse" as const, to: options.discoveryHref ?? "/", available: true },
    { view: "notebook" as const, label: "Notes", icon: "notebook" as const, to: "/workspace", available: true },
    { view: "compare" as const, label: "Compare", icon: "compare" as const, to: compareHref, available: compareIds.length > 0 },
    { view: "rera" as const, label: "RERA", icon: "rera" as const, to: reraHref, available: hasFocus },
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
  const uniqueIds = [...new Set(ids.map((id) => id.trim()).filter(Boolean))].slice(0, 4);
  if (uniqueIds.length < 2) return "/workspace/compare";
  const params = new URLSearchParams();
  params.set("ids", uniqueIds.join(","));
  if (focusId && uniqueIds.includes(focusId)) params.set("focus", focusId);
  return `/workspace/compare?${params.toString()}`;
}

export function workspaceBuyVsRentHref(propertyId?: string | null): string {
  return propertyId
    ? `/workspace/buy-vs-rent/${encodeURIComponent(propertyId)}`
    : "/workspace/buy-vs-rent";
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
