import { Link } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";

type WorkspaceIconName =
  | "browse"
  | "home"
  | "compare"
  | "plan"
  | "chevron";

export type WorkspaceView = "browse" | "home" | "compare" | "plan";

type WorkspaceSidebarProps = {
  homes: PropertyCard[];
  focusedId: string;
  compareHref: string;
  activeView: WorkspaceView;
  collapsed: boolean;
  onToggle: () => void;
  onFocus: (propertyId: string) => void;
  onRemove: (propertyId: string) => void;
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

  if (name === "browse") {
    return <svg {...common}><circle cx="11" cy="11" r="6" /><path d="m16 16 4 4" /></svg>;
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
  return <svg {...common}><path d="m9 18 6-6-6-6" /></svg>;
}

function workspaceNavItems(
  focusedId: string,
  compareHref: string,
  activeView: WorkspaceView,
) {
  const detailHref = focusedId ? `/property/${focusedId}` : "/";
  const planHref = focusedId ? `/property/${focusedId}/plan` : "/";
  return [
    { view: "browse" as const, label: "Home", icon: "browse" as const, to: "/" },
    { view: "home" as const, label: "Detail", icon: "home" as const, to: detailHref },
    { view: "compare" as const, label: "Compare", icon: "compare" as const, to: compareHref },
    { view: "plan" as const, label: "Plan", icon: "plan" as const, to: planHref },
  ].map((item) => ({
    ...item,
    active: item.view === activeView,
  }));
}

function societyLabel(home: PropertyCard): string {
  return home.society_name?.trim() || home.title;
}

function formatCompactPrice(price: number): string {
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(2)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function homeStateHint(home: PropertyCard): string | null {
  return home.home_state_display
    || home.project_status_display
    || null;
}

export function WorkspaceSidebar({
  homes,
  focusedId,
  compareHref,
  activeView,
  collapsed,
  onToggle,
  onFocus,
  onRemove,
}: WorkspaceSidebarProps) {
  const navItems = workspaceNavItems(focusedId, compareHref, activeView);
  const canRemove = homes.length > 2;

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

      <nav className="workspace-sidebar__nav" aria-label="Workspace">
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
          <div className="workspace-sidebar__shortlist-head">
            <h2 id="workspace-shortlist-title">Active selection</h2>
            <span>{homes.length}</span>
          </div>
          <div className="workspace-sidebar__shortlist-list">
            {homes.map((home) => {
              const name = societyLabel(home);
              const state = homeStateHint(home);
              return (
                <div
                  key={home.id}
                  className={`workspace-sidebar__home${home.id === focusedId ? " is-active" : ""}`}
                >
                  <button
                    type="button"
                    className="workspace-sidebar__home-open"
                    title={name}
                    onClick={() => onFocus(home.id)}
                  >
                    <strong>{name}</strong>
                    <span>
                      {home.area} · {home.bhk}BHK · {formatCompactPrice(home.price)}
                    </span>
                    {state && <em>{state}</em>}
                  </button>
                  {canRemove && (
                    <button
                      type="button"
                      className="workspace-sidebar__home-remove"
                      aria-label={`Remove ${name} from shortlist`}
                      title="Remove"
                      onClick={() => onRemove(home.id)}
                    >
                      ×
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}

      <div className="workspace-sidebar__footer">
        <span>OE</span>
        {!collapsed && <p>Workspace</p>}
      </div>
    </aside>
  );
}
