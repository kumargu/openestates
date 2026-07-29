import { useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";

type WorkspaceIconName =
  | "browse"
  | "home"
  | "notebook"
  | "plan"
  | "chevron";

export type WorkspaceView = "browse" | "home" | "notebook" | "compare" | "plan";

type WorkspaceSidebarProps = {
  homes: PropertyCard[];
  focusedId: string;
  activeView: WorkspaceView;
  collapsed: boolean;
  reduced: boolean;
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
  if (name === "notebook") {
    return (
      <svg {...common}>
        <path d="M7 3.5h8.5A2.5 2.5 0 0 1 18 6v14.2l-5.2-2.6L7.5 20.2V6A2.5 2.5 0 0 1 10 3.5" />
      </svg>
    );
  }
  if (name === "plan") {
    return <svg {...common}><path d="M5 3h14v18H5zM8 8h8M8 12h8M8 16h4" /></svg>;
  }
  return <svg {...common}><path d="m9 18 6-6-6-6" /></svg>;
}

function workspaceNavItems(
  focusedId: string,
  activeView: WorkspaceView,
) {
  const detailHref = focusedId ? `/property/${focusedId}` : "/";
  const planHref = focusedId ? `/property/${focusedId}/plan` : "/";
  return [
    { view: "browse" as const, label: "Discover", icon: "browse" as const, to: "/" },
    { view: "home" as const, label: "Property", icon: "home" as const, to: detailHref },
    { view: "notebook" as const, label: "Workspace", icon: "notebook" as const, to: "/workspace" },
    { view: "plan" as const, label: "Financial plan", icon: "plan" as const, to: planHref },
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
  activeView,
  collapsed,
  reduced,
  onToggle,
  onFocus,
  onRemove,
}: WorkspaceSidebarProps) {
  const navItems = workspaceNavItems(focusedId, activeView);
  const [showAllHomes, setShowAllHomes] = useState(false);
  const { notes } = useNotebook();
  const noteCount = notes.length;
  const previewHomes = homes.slice(0, 4);
  const focusedHome = homes.find((home) => home.id === focusedId);
  if (focusedHome && !previewHomes.some((home) => home.id === focusedHome.id)) {
    previewHomes[previewHomes.length - 1] = focusedHome;
  }
  const visibleHomes = showAllHomes ? homes : previewHomes;

  return (
    <aside className={`workspace-sidebar${collapsed ? " workspace-sidebar--collapsed" : ""}${reduced ? " workspace-sidebar--reduced" : ""}`}>
      <div className="workspace-sidebar__brand-row">
        <Link to="/" className="workspace-sidebar__brand" aria-label="OpenEstates home">
          <span>O</span>
          {!collapsed && <strong>OpenEstates</strong>}
        </Link>
        <button
          type="button"
          className="workspace-sidebar__toggle"
          aria-label={reduced ? "Shortlist opens after you save a home" : collapsed ? "Expand shortlist sidebar" : "Collapse shortlist sidebar"}
          aria-expanded={!collapsed}
          disabled={reduced}
          onClick={onToggle}
        >
          <span className={collapsed ? "" : "workspace-sidebar__toggle-icon--reversed"}>
            <WorkspaceIcon name="chevron" size={15} />
          </span>
        </button>
      </div>

      <nav className="workspace-sidebar__nav" aria-label="Buyer workspace">
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
            {!collapsed && item.view === "notebook" && noteCount > 0 && (
              <em className="workspace-sidebar__note-count">{noteCount}</em>
            )}
          </Link>
        ))}
      </nav>

      {!collapsed && (
        <section className="workspace-sidebar__shortlist" aria-labelledby="workspace-shortlist-title">
          <div className="workspace-sidebar__shortlist-head">
            <h2 id="workspace-shortlist-title">Shortlist</h2>
            <span>{homes.length}</span>
          </div>
          <div className="workspace-sidebar__shortlist-list">
            {visibleHomes.length === 0 && (
              <div className="workspace-sidebar__empty">
                <strong>Your shortlist is empty</strong>
                <p>Save homes you want to investigate or compare.</p>
              </div>
            )}
            {visibleHomes.map((home) => {
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
                  <button
                    type="button"
                    className="workspace-sidebar__home-remove"
                    aria-label={`Remove ${name} from shortlist`}
                    title="Remove"
                    onClick={() => onRemove(home.id)}
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
          {homes.length > previewHomes.length && (
            <button
              type="button"
              className="workspace-sidebar__shortlist-toggle"
              aria-expanded={showAllHomes}
              onClick={() => setShowAllHomes((current) => !current)}
            >
              {showAllHomes ? "Show fewer" : "… More"}
            </button>
          )}
        </section>
      )}

      <div className="workspace-sidebar__footer" aria-hidden="true">
        <span>OE</span>
        {!collapsed && <p>Your shortlist</p>}
      </div>
    </aside>
  );
}
