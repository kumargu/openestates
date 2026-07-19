/**
 * Slide-over side panel for quick property preview.
 * Opens from the right, keeps results grid visible.
 */
import { Fragment, useCallback, useEffect, useState, useRef } from "react";
import { Link } from "react-router-dom";
import type { PropertyDetailResponse, PropertyCard as PropertyCardType } from "../lib/types.ts";
import { getProperty } from "../lib/api.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";
import { isSaved, toggleSaved } from "../lib/sheet-store.ts";
import { TrustBadge } from "./TrustBadge.tsx";
import { ProjectStatusTag } from "./ProjectStatusTag.tsx";
import { DataFreshnessBadge } from "./DataFreshnessBadge.tsx";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";
import { evidenceReceiptLabel, summarizeEvidence, topEvidenceGlance } from "../lib/evidence.ts";

function formatPrice(price: number): string {
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
  return lowered.length > 0 && lowered !== "not specified" && lowered !== "unknown" && lowered !== "n/a";
}

type Props = {
  propertyId: string;
  card: PropertyCardType;
  onClose: () => void;
  onSaveChange?: () => void;
};

export function PropertySidePanel({ propertyId, card, onClose, onSaveChange }: Props) {
  const [detail, setDetail] = useState<PropertyDetailResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [saved, setSaved] = useState(() => isSaved(propertyId));
  const [closing, setClosing] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const specs = [
    { value: card.bhk.toString(), label: "BHK" },
    hasKnownNumber(card.sqft) ? { value: card.sqft.toLocaleString("en-IN"), label: "sqft" } : null,
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

  const handleSave = () => {
    setSaved(toggleSaved(propertyId));
    onSaveChange?.();
  };

  const evidenceSummary = summarizeEvidence(detail?.evidence);
  const evidenceGlance = topEvidenceGlance(detail?.evidence, 2);

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
          <button className="side-panel-close" onClick={handleClose} aria-label="Close panel">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
              <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
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
                const isSellerRegistered = tag === "seller-registered";
                const isVerificationPending = tag === "verification-pending";
                const tagStyle = isSellerRegistered
                  ? { background: "rgba(251, 191, 36, 0.85)", color: "#78350f" }
                  : isVerificationPending
                  ? { background: "rgba(156, 163, 175, 0.85)", color: "#fff" }
                  : undefined;
                return (
                  <span key={tag} className="side-panel-hero-tag" style={tagStyle}>
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
          <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap", margin: "0.35rem 0 0.5rem" }}>
            <TrustBadge rootSource={card.root_source} compact />
            <ProjectStatusTag
              status={detail?.project_status}
              displayText={detail?.project_status_display}
              possessionStatus={card.possession_status}
            />
            <DataFreshnessBadge freshness={card.data_freshness ?? detail?.data_freshness} compact />
          </div>

          <div className="side-panel-price-row">
            <span className="side-panel-price">{formatPrice(card.price)}</span>
            {hasKnownNumber(card.price_per_sqft) && (
              <span className="side-panel-ppsqft">{card.price_per_sqft.toLocaleString("en-IN")} /sqft</span>
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
              <div className="side-panel-loading-bar" style={{ width: "60%" }} />
            </div>
          )}

          {!loading && (
            <div className="side-panel-skim">
              {card.home_state_display && (
                <span className="side-panel-skim__chip">{card.home_state_display}</span>
              )}
              {card.builder_delivery_display && (
                <span className="side-panel-skim__chip">{card.builder_delivery_display}</span>
              )}
              {evidenceSummary && (
                <p className="side-panel-skim__proof">
                  {evidenceReceiptLabel(evidenceSummary)}
                </p>
              )}
              {evidenceGlance.map((line) => (
                <p key={line} className="side-panel-skim__line">{line}</p>
              ))}
            </div>
          )}
        </div>

        {/* Sticky footer actions */}
        <div className="side-panel-footer">
          <button
            className={`side-panel-save-btn ${saved ? "side-panel-save-btn--saved" : ""}`}
            onClick={handleSave}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill={saved ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
            </svg>
            {saved ? "Saved" : "Save"}
          </button>
          <Link to={`/property/${propertyId}`} className="side-panel-full-btn">
            Full details
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="5" y1="12" x2="19" y2="12" /><polyline points="12 5 19 12 12 19" />
            </svg>
          </Link>
          <Link to={`/property/${propertyId}/plan`} className="side-panel-plan-btn">
            {BUY_VS_RENT.short}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M5 12h14M13 6l6 6-6 6" />
            </svg>
          </Link>
        </div>
      </div>
    </div>
  );
}
