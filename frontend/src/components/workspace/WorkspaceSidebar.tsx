import { useState } from "react";
import { Link } from "react-router-dom";
import type { PropertyCard } from "../../lib/types.ts";
import { useNotebook } from "../../hooks/useNotebook.ts";
import {
  workspaceNavItems,
  type WorkspaceView,
} from "../../lib/workspaceNav.ts";
import { OpenEstatesMark } from "../brand/OpenEstatesMark.tsx";
import { requestDiscoveryReturn } from "../../lib/navigationContext.ts";

type WorkspaceIconName =
  | "browse"
  | "listing"
  | "notebook"
  | "compare"
  | "rera"
  | "saved";

type WorkspaceSidebarProps = {
  homes: PropertyCard[];
  focusedId: string;
  activeView: WorkspaceView;
  collapsed: boolean;
  reduced: boolean;
  mode: "discovery" | "property-context" | "workspace";
  discoveryHref: string;
  onToggle: () => void;
  onFocus: (propertyId: string) => void;
  onRemove: (propertyId: string) => void;
};

function WorkspaceIcon({
  name,
  size = 20,
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
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  if (name === "browse") {
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="8" />
        <path d="m16 8-2.6 5.4L8 16l2.6-5.4Z" />
      </svg>
    );
  }
  if (name === "listing") {
    return (
      <svg {...common}>
        <path d="M4.5 10.2 12 4.25l7.5 5.95V19a1.5 1.5 0 0 1-1.5 1.5H6A1.5 1.5 0 0 1 4.5 19Z" />
        <path d="M9.5 20.5v-6h5v6" />
      </svg>
    );
  }
  if (name === "notebook") {
    return (
      <svg {...common}>
        <path d="M8 4.75h7.25A1.75 1.75 0 0 1 17 6.5v13H8A1.75 1.75 0 0 1 6.25 17.75v-11A2 2 0 0 1 8 4.75Z" />
        <path d="M9.75 9.25h4.5M9.75 12.5h4.5M9.75 15.75h3" />
      </svg>
    );
  }
  if (name === "compare") {
    return (
      <svg {...common}>
        <rect x="3.5" y="5" width="7" height="14" rx="1.4" />
        <rect x="13.5" y="5" width="7" height="14" rx="1.4" />
      </svg>
    );
  }
  if (name === "rera") {
    return (
      <svg {...common}>
        <path d="M12 3.75 18.25 6.2v5.3c0 4-2.5 6.7-6.25 8.75C8.25 18.2 5.75 15.5 5.75 11.5V6.2Z" />
        <path d="m9.15 12 1.85 1.85 3.85-4" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <rect x="4.5" y="5" width="6.25" height="14" rx="1.3" />
      <path d="M13.25 8.5h6.25M13.25 12h6.25M13.25 15.5h4.25" />
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

export function WorkspaceSidebar({
  homes,
  focusedId,
  activeView,
  collapsed,
  reduced,
  mode,
  discoveryHref,
  onToggle,
  onFocus,
  onRemove,
}: WorkspaceSidebarProps) {
  const focusedHome = homes.find((home) => home.id === focusedId);
  const navItems = workspaceNavItems(focusedId, activeView, {
    mode,
    discoveryHref,
    compareIds: homes.map((home) => home.id),
  });
  const [showAllHomes, setShowAllHomes] = useState(false);
  const { notes } = useNotebook();
  const noteCount = notes.length;
  const shortlistHomes = mode === "property-context"
    ? homes.filter((home) => home.id !== focusedId)
    : homes;
  const previewHomes = shortlistHomes.slice(0, 4);
  if (
    mode !== "property-context"
    && focusedHome
    && !previewHomes.some((home) => home.id === focusedHome.id)
  ) {
    previewHomes[previewHomes.length - 1] = focusedHome;
  }
  const visibleHomes = showAllHomes ? shortlistHomes : previewHomes;

  return (
    <aside
      className={`workspace-sidebar workspace-sidebar--${mode}${collapsed ? " workspace-sidebar--collapsed" : ""}${reduced ? " workspace-sidebar--reduced" : ""}`}
    >
      <div className="workspace-sidebar__brand-row">
        <Link to="/" className="workspace-sidebar__brand" aria-label="OpenEstates home">
          <OpenEstatesMark size={28} className="workspace-sidebar__mark" />
          {!collapsed && <strong>OpenEstates</strong>}
        </Link>
      </div>

      <nav className="workspace-sidebar__nav" aria-label={mode === "property-context" ? "Property navigation" : "Buyer workspace"}>
        {navItems.map((item) => {
          const title = !item.available ? `${item.label} — save a home first` : undefined;
          const badge = item.view === "notebook" && noteCount > 0 ? noteCount : null;
          const body = (
            <>
              <span className="workspace-sidebar__nav-icon">
                <WorkspaceIcon name={item.icon} />
                {badge ? <em className="workspace-sidebar__nav-badge">{badge}</em> : null}
              </span>
              <span className="workspace-sidebar__nav-label">{item.label}</span>
            </>
          );

          if (!item.available) {
            return (
              <span
                key={item.label}
                className="workspace-sidebar__nav-item is-disabled"
                aria-disabled="true"
                title={title}
              >
                {body}
              </span>
            );
          }

          return (
            <Link
              key={item.label}
              to={item.to}
              className={`workspace-sidebar__nav-item${item.active ? " is-active" : ""}`}
              aria-current={item.active ? "page" : undefined}
              title={title}
              onClick={() => {
                if (item.view === "browse") requestDiscoveryReturn(item.to);
              }}
            >
              {body}
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
            <h2 id="workspace-shortlist-title">
              {mode === "property-context" ? "Other saved homes" : "Shortlist"}
            </h2>
            <span>{shortlistHomes.length}</span>
          </div>
          <div className="workspace-sidebar__shortlist-list">
            {visibleHomes.length === 0 && (
              <div className="workspace-sidebar__empty">
                <strong>
                  {mode === "property-context"
                    ? "No other saved homes"
                    : "Your shortlist is empty"}
                </strong>
                <p>Save another home to compare.</p>
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
                      {[home.area, home.bhk > 0 ? `${home.bhk}BHK` : null]
                        .filter(Boolean)
                        .join(" · ")} · {formatCompactPrice(home.price)}
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
          {shortlistHomes.length > previewHomes.length && (
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

      <div className="workspace-sidebar__footer">
        <button
          type="button"
          className={`workspace-sidebar__nav-item workspace-sidebar__saved${collapsed ? "" : " is-open"}`}
          aria-label={
            reduced
              ? "Save a home to open the shortlist"
              : collapsed
                ? "Show saved homes"
                : "Hide saved homes"
          }
          aria-expanded={!collapsed}
          disabled={reduced}
          onClick={onToggle}
        >
          <span className="workspace-sidebar__nav-icon">
            <WorkspaceIcon name="saved" />
            {homes.length > 0 ? (
              <em className="workspace-sidebar__nav-badge">{homes.length}</em>
            ) : null}
          </span>
          <span className="workspace-sidebar__nav-label">Saved</span>
        </button>
      </div>
    </aside>
  );
}
