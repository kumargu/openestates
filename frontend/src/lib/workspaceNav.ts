export type WorkspaceView = "browse" | "home" | "notebook" | "compare" | "rera" | "plan";

export type WorkspaceNavItem = {
  view: WorkspaceView;
  label: string;
  icon: "browse" | "listing" | "notebook" | "rera" | "plan";
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
    mode?: "property-context" | "workspace";
    propertyLabel?: string;
    discoveryHref?: string;
  } = {},
): WorkspaceNavItem[] {
  const encodedId = focusedId ? encodeURIComponent(focusedId) : "";
  const hasFocus = Boolean(encodedId);
  const detailHref = hasFocus ? `/property/${encodedId}` : "/";
  const reraHref = hasFocus ? `/property/${encodedId}/rera` : "/";

  const mode = options.mode ?? "workspace";
  if (mode === "property-context") {
    return [
      {
        view: "browse",
        label: options.discoveryHref && options.discoveryHref !== "/" ? "Back to results" : "Explore homes",
        icon: "browse",
        to: options.discoveryHref ?? "/",
        active: false,
        available: true,
      },
      {
        view: "home",
        label: options.propertyLabel?.trim() || "Property overview",
        icon: "listing",
        to: detailHref,
        active: activeView === "home",
        available: hasFocus,
      },
      { view: "rera", label: "RERA evidence", icon: "rera", to: reraHref, active: activeView === "rera", available: hasFocus },
      {
        view: "plan",
        label: "Buy vs Rent",
        icon: "plan",
        to: hasFocus ? workspaceBuyVsRentHref(focusedId) : "/workspace/buy-vs-rent",
        active: false,
        available: hasFocus,
      },
      { view: "notebook", label: "Workspace", icon: "notebook", to: "/workspace", active: false, available: true },
    ];
  }

  return [
    { view: "browse" as const, label: "Add homes", icon: "browse" as const, to: options.discoveryHref ?? "/", available: true },
    { view: "notebook" as const, label: "Workspace", icon: "notebook" as const, to: "/workspace", available: true },
  ].map((item) => ({
    ...item,
    active: item.view === "notebook"
      ? ["notebook", "compare", "plan"].includes(activeView)
      : item.view === activeView,
  }));
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
