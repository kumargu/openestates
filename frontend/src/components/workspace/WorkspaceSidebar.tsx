import { Link } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";

type WorkspaceIconName =
  | "discover"
  | "home"
  | "compare"
  | "plan"
  | "area"
  | "chevron";

export type WorkspaceView = "discover" | "home" | "compare" | "plan" | "area";

type WorkspaceSidebarProps = {
  homes: PropertyCard[];
  focusedId: string;
  compareHref: string;
  activeView: WorkspaceView;
  collapsed: boolean;
  onToggle: () => void;
  onFocus: (propertyId: string) => void;
};

function WorkspaceIcon({ name, size = 17 }: { name: WorkspaceIconName; size?: number }) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  if (name === "discover") {
    return <svg {...common}><circle cx="11" cy="11" r="6" /><path d="m16 16 4 4M8.5 11h5M11 8.5v5" /></svg>;
  }
  if (name === "home") {
    return <svg {...common}><path d="m4 10 8-6 8 6v9H4z" /><path d="M9 19v-6h6v6" /></svg>;
  }
  if (name === "compare") {
    return <svg {...common}><path d="M7 4v16M17 4v16M4 8l3-3 3 3M14 16l3 3 3-3" /></svg>;
  }
  if (name === "plan") {
    return <svg {...common}><path d="M5 3h14v18H5zM8 8h8M8 12h8M8 16h4" /></svg>;
  }
  if (name === "area") {
    return <svg {...common}><path d="M12 21s6-5.2 6-11a6 6 0 1 0-12 0c0 5.8 6 11 6 11Z" /><circle cx="12" cy="10" r="2" /></svg>;
  }
  return <svg {...common}><path d="m9 18 6-6-6-6" /></svg>;
}

function workspaceNavItems(
  focusedId: string,
  compareHref: string,
  activeView: WorkspaceView,
) {
  const detailHref = focusedId ? `/property/${focusedId}` : "/results";
  const planHref = focusedId ? `/property/${focusedId}/plan` : "/results";
  return [
    { view: "discover" as const, label: "Discover", icon: "discover" as const, to: "/results" },
    { view: "home" as const, label: "Home detail", icon: "home" as const, to: detailHref },
    { view: "compare" as const, label: "Compare", icon: "compare" as const, to: compareHref },
    { view: "plan" as const, label: "Plan", icon: "plan" as const, to: planHref },
    { view: "area" as const, label: "Area Tracker", icon: "area" as const, to: "/#area-tracker" },
  ].map((item) => ({
    ...item,
    active: item.view === activeView,
  }));
}

export function WorkspaceSidebar({
  homes,
  focusedId,
  compareHref,
  activeView,
  collapsed,
  onToggle,
  onFocus,
}: WorkspaceSidebarProps) {
  const navItems = workspaceNavItems(focusedId, compareHref, activeView);

  return (
    <aside className={`workspace-sidebar${collapsed ? " workspace-sidebar--collapsed" : ""}`}>
      <div className="workspace-sidebar__brand-row">
        <Link to="/" className="workspace-sidebar__brand" aria-label="OpenEstates home">
          <span>O</span>
          {!collapsed && <strong>OpenEstates</strong>}
        </Link>
        <button
          type="button"
          className="workspace-sidebar__toggle"
          aria-label={collapsed ? "Expand workspace sidebar" : "Collapse workspace sidebar"}
          aria-expanded={!collapsed}
          onClick={onToggle}
        >
          <span className={collapsed ? "" : "workspace-sidebar__toggle-icon--reversed"}>
            <WorkspaceIcon name="chevron" size={15} />
          </span>
        </button>
      </div>

      <nav className="workspace-sidebar__nav" aria-label="Decision workspace">
        {navItems.map((item) => (
          <Link
            key={item.label}
            to={item.to}
            className={`workspace-sidebar__nav-item${item.active ? " is-active" : ""}`}
            aria-current={item.active ? "page" : undefined}
            title={collapsed ? item.label : undefined}
          >
            <WorkspaceIcon name={item.icon} />
            {!collapsed && <span>{item.label}</span>}
            {!collapsed && item.view === "compare" && <em>{homes.length}</em>}
          </Link>
        ))}
      </nav>

      {!collapsed && (
        <section className="workspace-sidebar__shortlist" aria-labelledby="workspace-shortlist-title">
          <h2 id="workspace-shortlist-title">Compared homes</h2>
          <div>
            {homes.map((home) => (
              <button
                key={home.id}
                type="button"
                className={home.id === focusedId ? "is-active" : ""}
                onClick={() => onFocus(home.id)}
              >
                <strong>{home.title}</strong>
                <span>{home.area} · {formatCompactPrice(home.price)}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      <div className="workspace-sidebar__footer">
        <span>OE</span>
        {!collapsed && <p>Decision workspace</p>}
      </div>
    </aside>
  );
}

function formatCompactPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(2)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}
