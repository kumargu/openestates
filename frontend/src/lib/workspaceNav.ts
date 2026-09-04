import {
  hrefWithSearchSpan,
  propertyHrefWithSearchSpan,
  type SearchSpanReference,
  type NavigationMode,
} from "./navigationContext.ts";

const MAX_ACTIVE_COMPARE_HOMES = 4;

export type WorkspaceView = "browse" | "home" | "notebook" | "compare" | "rera" | "plan";

export type WorkspaceNavItem = {
  view: WorkspaceView;
  label: string;
  icon: "back" | "browse" | "listing" | "notebook" | "rera" | "plan";
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

function discoveryReturnLabel(resultCount?: number): string {
  if (!resultCount || resultCount < 1) return "Back to results";
  return `Back to ${resultCount} ${resultCount === 1 ? "result" : "results"}`;
}

export function workspaceNavItems(
  focusedId: string,
  activeView: WorkspaceView,
  options: {
    discoveryHref?: string;
    discoveryResultCount?: number;
    hasDiscoveryContext?: boolean;
    propertySearchContext?: SearchSpanReference | null;
  } = {},
): WorkspaceNavItem[] {
  const encodedId = focusedId ? encodeURIComponent(focusedId) : "";
  const hasFocus = Boolean(encodedId);
  const detailHref = hasFocus
    ? propertyHrefWithSearchSpan(focusedId, options.propertySearchContext ?? null)
    : "/";
  const reraHref = hasFocus
    ? propertyHrefWithSearchSpan(focusedId, options.propertySearchContext ?? null, "/rera")
    : "/";
  const planHref = hrefWithSearchSpan(
    workspaceBuyVsRentHref(focusedId),
    options.propertySearchContext,
  );
  const notebookParams = new URLSearchParams();
  if (focusedId) notebookParams.set("focus", focusedId);
  const notebookHref = hrefWithSearchSpan(
    notebookParams.size > 0
      ? `/workspace?${notebookParams.toString()}`
      : "/workspace",
    options.propertySearchContext,
  );
  const canReturnToDiscovery = activeView !== "browse" && options.hasDiscoveryContext;
  return [
    {
      view: "browse",
      label: canReturnToDiscovery
        ? discoveryReturnLabel(options.discoveryResultCount)
        : "Explore",
      icon: canReturnToDiscovery ? "back" : "browse",
      to: options.discoveryHref ?? "/",
      active: activeView === "browse",
      available: true,
    },
    {
      view: "home",
      label: "This property",
      icon: "listing",
      to: detailHref,
      active: activeView === "home",
      available: hasFocus,
    },
    { view: "rera", label: "RERA", icon: "rera", to: reraHref, active: activeView === "rera", available: hasFocus },
    { view: "plan", label: "EMI Plan", icon: "plan", to: planHref, active: activeView === "plan", available: hasFocus },
    {
      view: "notebook",
      label: "Workspace",
      icon: "notebook",
      to: notebookHref,
      active: activeView === "notebook" || activeView === "compare",
      available: true,
    },
  ];
}

export function shouldShowWorkspaceSidebar(
  mode: NavigationMode,
  savedHomeCount: number,
): boolean {
  if (mode === "property-context" || mode === "workspace") return true;
  return savedHomeCount > 0;
}

export function workspaceCompareHref(ids: string[], focusId?: string): string {
  const uniqueIds = activeWorkspaceCompareIds(ids, []);
  if (uniqueIds.length < 2) return "/workspace/compare";
  const params = new URLSearchParams();
  params.set("ids", uniqueIds.join(","));
  if (focusId && uniqueIds.includes(focusId)) params.set("focus", focusId);
  return `/workspace/compare?${params.toString()}`;
}

/**
 * Resolve the one active comparison set shared by deep links and local workspace state.
 * A URL selection wins when present; otherwise the buyer's persisted selection is used.
 */
export function activeWorkspaceCompareIds(
  requestedIds: string[],
  persistedIds: string[],
): string[] {
  const source = requestedIds.length > 0 ? requestedIds : persistedIds;
  return [...new Set(source.map((id) => id.trim()).filter(Boolean))]
    .slice(0, MAX_ACTIVE_COMPARE_HOMES);
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
