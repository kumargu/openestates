export type WorkspaceView = "browse" | "home" | "notebook" | "compare" | "rera" | "plan";

export type WorkspaceNavItem = {
  view: WorkspaceView;
  label: string;
  icon: "browse" | "home" | "notebook" | "rera" | "plan";
  to: string;
  active: boolean;
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
  const detailHref = encodedId ? `/property/${encodedId}` : "/";
  const reraHref = encodedId ? `/property/${encodedId}/rera` : "/";
  const planHref = encodedId ? `/property/${encodedId}/plan` : "/";
  return [
    { view: "browse" as const, label: "Discover", icon: "browse" as const, to: "/" },
    { view: "home" as const, label: "Property", icon: "home" as const, to: detailHref },
    { view: "notebook" as const, label: "Workspace", icon: "notebook" as const, to: "/workspace" },
    { view: "rera" as const, label: "RERA", icon: "rera" as const, to: reraHref },
    { view: "plan" as const, label: "Financial plan", icon: "plan" as const, to: planHref },
  ].map((item) => ({
    ...item,
    active: item.view === activeView,
  }));
}
