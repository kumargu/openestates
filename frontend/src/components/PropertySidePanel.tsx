/**
 * Slide-over side panel for quick property preview.
 * Opens from the right, keeps results grid visible.
 */
import { Fragment, useCallback, useEffect, useState, useRef } from "react";
import { Link } from "react-router-dom";
import type {
  PropertyDetailResponse,
  PropertyCard as PropertyCardType,
} from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { TrustBadge } from "./TrustBadge.tsx";
import { ProjectStatusTag } from "./ProjectStatusTag.tsx";
import { DataFreshnessBadge } from "./DataFreshnessBadge.tsx";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";
import { isRedundantHomeState } from "../lib/property-signals.ts";

function formatPrice(price: number): string {
  if (!hasKnownNumber(price)) return "Price unavailable";
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

function isKnownText(value: string | null | undefined): value is string {
  if (!value) return false;
  const lowered = value.trim().toLowerCase();
  return (
    lowered.length > 0 &&
    lowered !== "not specified" &&
    lowered !== "unknown" &&
    lowered !== "n/a"
  );
}

type Props = {
  propertyId: string;
  card: PropertyCardType;
  onClose: () => void;
};

export function PropertySidePanel({ propertyId, card, onClose }: Props) {
  const [detail, setDetail] = useState<PropertyDetailResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [closing, setClosing] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const specs = [
    { value: card.bhk.toString(), label: "BHK" },
    hasKnownNumber(card.sqft)
      ? { value: card.sqft.toLocaleString("en-IN"), label: "sqft" }
      : null,
    hasKnownNumber(card.floor) && hasKnownNumber(card.total_floors)
      ? { value: `${card.floor}/${card.total_floors}`, label: "Floor" }
      : null,
    isKnownText(card.facing) ? { value: card.facing, label: "Facing" } : null,
  ].filter((spec): spec is { value: string; label: string } => spec !== null);

  const handleClose = useCallback(() => {
    setClosing(true);
    setTimeout(onClose, 250);
  }, [onClose]);

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setLoading(true);
      setDetail(null);
    });
    getProperty(propertyId)
      .then((data) => {
        if (!cancelled) setDetail(data);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [propertyId]);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") handleClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [handleClose]);

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) handleClose();
  };

  const showHomeStateChip = Boolean(
    card.home_state_display &&
    !isRedundantHomeState(
      card.home_state_display,
      detail?.project_status_display,
      card.possession_status,
    ),
  );

  return (
    <div
      className={`side-panel-backdrop ${closing ? "side-panel-backdrop--closing" : ""}`}
      onClick={handleBackdropClick}
    >
      <div
        ref={panelRef}
        className={`side-panel ${closing ? "side-panel--closing" : ""}`}
        role="dialog"
        aria-label={`Property details: ${card.title}`}
      >
        {/* Header */}
        <div className="side-panel-header">
          <button
            className="side-panel-close"
            onClick={handleClose}
            aria-label="Close panel"
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
          <span className="side-panel-header-label">Quick view</span>
        </div>

        {/* Hero image */}
        <div className="side-panel-hero">
          <ImageWithFallback
            src={card.hero_image}
            alt={card.title}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
          />
          {card.transparency_tags.length > 0 && (
            <div className="side-panel-hero-tags">
              {card.transparency_tags.slice(0, 3).map((tag) => {
                const isVerificationPending = tag === "verification-pending";
                const tagStyle = isVerificationPending
                  ? { background: "rgba(156, 163, 175, 0.85)", color: "#fff" }
                  : undefined;
                return (
                  <span
                    key={tag}
                    className="side-panel-hero-tag"
                    style={tagStyle}
                  >
                    {tag.replace(/_/g, " ").replace(/-/g, " ")}
                  </span>
                );
              })}
            </div>
          )}
        </div>

        {/* Scrollable content */}
        <div className="side-panel-body">
          {/* Title + price */}
          <h2 className="side-panel-title">{card.title}</h2>
          <p className="side-panel-location">
            {card.society_name} &middot; {card.area}
          </p>

          {/* Trust badges */}
          <div
            style={{
              display: "flex",
              gap: "0.35rem",
              flexWrap: "wrap",
              margin: "0.35rem 0 0.5rem",
            }}
          >
            <TrustBadge rootSource={card.root_source} compact />
            <ProjectStatusTag
              status={detail?.project_status}
              displayText={detail?.project_status_display}
              possessionStatus={card.possession_status}
            />
            <DataFreshnessBadge
              freshness={card.data_freshness ?? detail?.data_freshness}
              compact
            />
          </div>

          <div className="side-panel-price-row">
            <span className="side-panel-price">{formatPrice(card.price)}</span>
            {hasKnownNumber(card.price_per_sqft) && (
              <span className="side-panel-ppsqft">
                {card.price_per_sqft.toLocaleString("en-IN")} /sqft
              </span>
            )}
          </div>

          <div className="side-panel-specs">
            {specs.map((spec, index) => (
              <Fragment key={spec.label}>
                {index > 0 && <div className="side-panel-spec-divider" />}
                <div className="side-panel-spec">
                  <span className="side-panel-spec-value">{spec.value}</span>
                  <span className="side-panel-spec-label">{spec.label}</span>
                </div>
              </Fragment>
            ))}
          </div>

          {loading && (
            <div className="side-panel-loading">
              <div className="side-panel-loading-bar" />
              <div
                className="side-panel-loading-bar"
                style={{ width: "60%" }}
              />
            </div>
          )}

          {!loading && (
            <div className="side-panel-skim">
              {showHomeStateChip && (
                <span className="side-panel-skim__chip">
                  {card.home_state_display}
                </span>
              )}
              {card.builder_delivery_display && (
                <span className="side-panel-skim__chip">
                  {card.builder_delivery_display}
                </span>
              )}
            </div>
          )}
        </div>

        {/* Sticky footer actions */}
        <div className="side-panel-footer">
          <Link to={`/property/${propertyId}`} className="side-panel-full-btn">
            Full details
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <line x1="5" y1="12" x2="19" y2="12" />
              <polyline points="12 5 19 12 12 19" />
            </svg>
          </Link>
          <Link
            to={`/property/${propertyId}/plan`}
            className="side-panel-plan-btn"
          >
            {BUY_VS_RENT.short}
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M5 12h14M13 6l6 6-6 6" />
            </svg>
          </Link>
        </div>
      </div>
    </div>
  );
}
