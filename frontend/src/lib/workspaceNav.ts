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
  if (/^\/property\/[^/]+\/rera$/.test(pathname)) return "rera";
  if (/^\/property\/[^/]+\/plan$/.test(pathname)) return "plan";
  if (/^\/property\/[^/]+$/.test(pathname)) return "home";
  return "browse";
}

export function workspaceNavItems(
  focusedId: string,
  activeView: WorkspaceView,
): WorkspaceNavItem[] {
  const encodedId = focusedId ? encodeURIComponent(focusedId) : "";
  const hasFocus = Boolean(encodedId);
  const detailHref = hasFocus ? `/property/${encodedId}` : "/";
  const reraHref = hasFocus ? `/property/${encodedId}/rera` : "/";
  const planHref = hasFocus ? `/property/${encodedId}/plan` : "/";

  // Journey: Search → This home → Workspace → RERA → Plan.
  // "This home" is the focused shortlist listing — not the app home.
  return [
    { view: "browse" as const, label: "Search", icon: "browse" as const, to: "/", available: true },
    {
      view: "home" as const,
      label: "This home",
      icon: "listing" as const,
      to: detailHref,
      available: hasFocus,
    },
    { view: "notebook" as const, label: "Workspace", icon: "notebook" as const, to: "/workspace", available: true },
    { view: "rera" as const, label: "RERA", icon: "rera" as const, to: reraHref, available: hasFocus },
    {
      view: "plan" as const,
      label: "Plan",
      icon: "plan" as const,
      to: planHref,
      available: hasFocus,
    },
  ].map((item) => ({
    ...item,
    active: item.view === activeView,
  }));
}
