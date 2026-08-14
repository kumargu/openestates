import { useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";
import {
  workspaceNavItems,
  type WorkspaceView,
} from "../../lib/workspaceNav.ts";
import { OpenEstatesMark } from "../brand/OpenEstatesMark.tsx";

type WorkspaceIconName =
  | "browse"
  | "listing"
  | "notebook"
  | "compare"
  | "rera"
  | "chevron";

type WorkspaceSidebarProps = {
  homes: PropertyCard[];
  focusedId: string;
  activeView: WorkspaceView;
  collapsed: boolean;
  reduced: boolean;
  mode: "discovery" | "property-context" | "workspace";
  onToggle: () => void;
  onFocus: (propertyId: string) => void;
  onRemove: (propertyId: string) => void;
};

function WorkspaceIcon({
  name,
  size = 17,
}: {
  name: WorkspaceIconName;
  size?: number;
}) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.65,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  if (name === "browse") {
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="8.25" />
        <path d="m15.35 8.65-2.2 4.5-4.5 2.2 2.2-4.5 4.5-2.2Z" />
      </svg>
    );
  }
  if (name === "listing") {
    return (
      <svg {...common}>
        <path d="m4 10 8-6.5 8 6.5v9.25a1.25 1.25 0 0 1-1.25 1.25H5.25A1.25 1.25 0 0 1 4 19.25Z" />
        <path d="M9 20.5v-6h6v6" />
      </svg>
    );
  }
  if (name === "notebook") {
    return (
      <svg {...common}>
        <path d="M6.5 3.75h7l4 4v12.5H6.5Z" />
        <path d="M13.5 3.75v4h4M9.25 12h5.5M9.25 15.5h4" />
      </svg>
    );
  }
  if (name === "compare") {
    return (
      <svg {...common}>
        <rect x="3.75" y="5.25" width="16.5" height="13.5" rx="2" />
        <path d="M12 5.25v13.5" />
      </svg>
    );
  }
  if (name === "rera") {
    return (
      <svg {...common}>
        <path d="M12 3.5 18.5 6v5.5c0 4.2-2.6 7-6.5 9-3.9-2-6.5-4.8-6.5-9V6Z" />
        <path d="m9.2 11.9 1.75 1.75 3.9-4.1" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

function societyLabel(home: PropertyCard): string {
  return home.society_name?.trim() || home.title;
}

function formatCompactPrice(price: number): string {
  if (price <= 0 || !Number.isFinite(price)) return "Price unavailable";
  if (price >= 10_000_000) return `₹${(price / 10_000_000).toFixed(2)} Cr`;
  if (price >= 100_000) return `₹${(price / 100_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

function homeStateHint(home: PropertyCard): string | null {
  return home.home_state_display || home.project_status_display || null;
}

function homeMeta(home: PropertyCard): string {
  return [home.area.trim(), `${home.bhk}BHK`, formatCompactPrice(home.price)]
    .filter(Boolean)
    .join(" · ");
}

export function WorkspaceSidebar({
  homes,
  focusedId,
  activeView,
  collapsed,
  reduced,
  mode,
  onToggle,
  onFocus,
  onRemove,
}: WorkspaceSidebarProps) {
  const { notes, compareIds } = useNotebook();
  const focusedHome = homes.find((home) => home.id === focusedId);
  const navItems = workspaceNavItems(focusedId, activeView, {
    mode,
    compareIds,
  });
  const [showAllHomes, setShowAllHomes] = useState(false);
  const noteCount = notes.length;
  const previewHomes = homes.slice(0, 4);
  if (focusedHome && !previewHomes.some((home) => home.id === focusedHome.id)) {
    previewHomes[previewHomes.length - 1] = focusedHome;
  }
  const visibleHomes = showAllHomes ? homes : previewHomes;

  return (
    <aside
      className={`workspace-sidebar workspace-sidebar--${mode}${collapsed ? " workspace-sidebar--collapsed" : ""}${reduced ? " workspace-sidebar--reduced" : ""}`}
    >
      <div className="workspace-sidebar__brand-row">
        <Link to="/" className="workspace-sidebar__brand" aria-label="OpenEstates home">
          <OpenEstatesMark size={26} className="workspace-sidebar__mark" />
          {!collapsed && <strong>OpenEstates</strong>}
        </Link>
        {mode === "workspace" ? (
          <button
            type="button"
            className="workspace-sidebar__toggle"
            aria-label={
              reduced
                ? "Shortlist opens after you save a home"
                : collapsed
                  ? "Expand shortlist sidebar"
                  : "Collapse shortlist sidebar"
            }
            aria-expanded={!collapsed}
            disabled={reduced}
            onClick={onToggle}
          >
            <span
              className={
                collapsed ? "" : "workspace-sidebar__toggle-icon--reversed"
              }
            >
              <WorkspaceIcon name="chevron" size={15} />
            </span>
          </button>
        ) : null}
      </div>

      <nav className="workspace-sidebar__nav" aria-label={mode === "property-context" ? "Property navigation" : "Buyer workspace"}>
        {navItems.map((item) => {
          const title = !item.available
            ? `${item.label} — save a home first`
            : collapsed
              ? item.label
              : undefined;

          if (!item.available) {
            return (
              <span
                key={item.label}
                className="workspace-sidebar__nav-item is-disabled"
                aria-disabled="true"
                title={title}
              >
                <WorkspaceIcon name={item.icon} />
                {!collapsed && <span>{item.label}</span>}
              </span>
            );
          }

          return (
            <Link
              key={item.label}
              to={item.to}
              className={`workspace-sidebar__nav-item${item.active ? " is-active" : ""}`}
              aria-label={item.label}
              aria-current={item.active ? "page" : undefined}
              title={title}
            >
              <WorkspaceIcon name={item.icon} />
              {!collapsed && <span>{item.label}</span>}
              {!collapsed && item.view === "notebook" && noteCount > 0 && (
                <em className="workspace-sidebar__note-count">{noteCount}</em>
              )}
            </Link>
          );
        })}
      </nav>

      {!collapsed && (
        <section
          className="workspace-sidebar__shortlist"
          aria-labelledby="workspace-shortlist-title"
        >
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
                    <span>{homeMeta(home)}</span>
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
        {!collapsed && <p>{mode === "property-context" ? "Property guide" : "Your shortlist"}</p>}
      </div>
    </aside>
  );
}
