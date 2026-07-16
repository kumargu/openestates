/**
 * Restrained line-icon set used as evidence anchors, not decoration.
 * All icons inherit `currentColor` and a shared 1.6 stroke for a calm,
 * consistent feel across the evidence stack.
 */
import type { CSSProperties } from "react";

type IconProps = { size?: number; style?: CSSProperties };

function base(size: number, style?: CSSProperties) {
  return {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.6,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    style,
    "aria-hidden": true,
  };
}

export function SealIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M12 3l7 3v5c0 4.4-3 8-7 10-4-2-7-5.6-7-10V6z" />
      <path d="M9 12l2 2 4-4" />
    </svg>
  );
}

export function TrendIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M3 17l6-6 4 4 8-8" />
      <path d="M21 7h-5" />
      <path d="M21 7v5" />
    </svg>
  );
}

export function PinIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M12 21s-6-5.2-6-10a6 6 0 0 1 12 0c0 4.8-6 10-6 10z" />
      <circle cx="12" cy="11" r="2.2" />
    </svg>
  );
}

export function TrainIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <rect x="6" y="4" width="12" height="12" rx="2.5" />
      <path d="M6 11h12" />
      <circle cx="9" cy="13.5" r="0.6" fill="currentColor" />
      <circle cx="15" cy="13.5" r="0.6" fill="currentColor" />
      <path d="M8 20l2-2M16 20l-2-2" />
    </svg>
  );
}

export function SchoolIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M12 5l9 4-9 4-9-4 9-4z" />
      <path d="M7 11v4c0 1 2.2 2.2 5 2.2s5-1.2 5-2.2v-4" />
    </svg>
  );
}

export function HospitalIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <rect x="4" y="4" width="16" height="16" rx="2.5" />
      <path d="M12 8v8M8 12h8" />
    </svg>
  );
}

export function TreeIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M12 3c3 0 5 2.4 5 5 2 .4 3 1.8 3 3.4 0 2-1.8 3.4-4 3.4H8c-2.2 0-4-1.5-4-3.6 0-1.7 1.2-3 3-3.3C7 5.3 9 3 12 3z" />
      <path d="M12 15v6" />
    </svg>
  );
}

export function RouteIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <circle cx="6" cy="18" r="2" />
      <circle cx="18" cy="6" r="2" />
      <path d="M8 18h6a3 3 0 0 0 0-6H10a3 3 0 0 1 0-6h6" />
    </svg>
  );
}

export function BuildingIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <rect x="5" y="3" width="14" height="18" rx="1.5" />
      <path d="M9 7h2M13 7h2M9 11h2M13 11h2M9 15h2M13 15h2" />
    </svg>
  );
}

export function QuoteIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M4 20l1.5-3.5A7 7 0 1 1 8.5 19 6.9 6.9 0 0 1 4 20z" />
    </svg>
  );
}

export function UsersIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <circle cx="9" cy="9" r="3" />
      <path d="M3.5 19a5.5 5.5 0 0 1 11 0" />
      <path d="M16 6.5a3 3 0 0 1 0 5.5" />
      <path d="M17.5 19a5.5 5.5 0 0 0-2-4.2" />
    </svg>
  );
}

export function AlertIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M12 4l9 15H3z" />
      <path d="M12 10v4M12 17h.01" />
    </svg>
  );
}

export function GapIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)} strokeDasharray="2.6 2.6">
      <circle cx="12" cy="12" r="8" />
    </svg>
  );
}

export function RupeeIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M8 5h8M8 9h8M15 5c0 4-3 5-7 5 3 0 5 2 6 5" />
    </svg>
  );
}

export function LinkIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M9.5 14.5l5-5" />
      <path d="M7 12l-1.5 1.5a3.2 3.2 0 0 0 4.5 4.5L12 16.5" />
      <path d="M17 12l1.5-1.5a3.2 3.2 0 0 0-4.5-4.5L12 7.5" />
    </svg>
  );
}

export function ChevronIcon({ size = 16, style }: IconProps) {
  return (
    <svg {...base(size, style)}>
      <path d="M6 9l6 6 6-6" />
    </svg>
  );
}

/** Map an evidence section kind to its lead icon component. */
export function IconForKind({ kind, size = 16 }: { kind: string; size?: number }) {
  switch (kind) {
    case "rera":
      return <SealIcon size={size} />;
    case "market":
      return <TrendIcon size={size} />;
    case "area":
      return <PinIcon size={size} />;
    case "nearby":
      return <RouteIcon size={size} />;
    case "reviews":
      return <QuoteIcon size={size} />;
    case "community":
      return <UsersIcon size={size} />;
    default:
      return <BuildingIcon size={size} />;
  }
}

/** Keyword-match a fact/label to an intuitive anchor icon component. */
export function IconForLabel({ label, size = 15 }: { label: string; size?: number }) {
  const l = label.toLowerCase();
  if (/(metro|train|station|rail|purple line|green line)/.test(l)) return <TrainIcon size={size} />;
  if (/(school|college|education|academ)/.test(l)) return <SchoolIcon size={size} />;
  if (/(hospital|clinic|health|medical)/.test(l)) return <HospitalIcon size={size} />;
  if (/(park|green|tree|lake|garden)/.test(l)) return <TreeIcon size={size} />;
  if (/(traffic|road|commute|route|highway|drive)/.test(l)) return <RouteIcon size={size} />;
  if (/(price|rate|cost|value|₹|budget|sqft)/.test(l)) return <RupeeIcon size={size} />;
  if (/(rera|registration|approv|legal|complaint|escrow)/.test(l)) return <SealIcon size={size} />;
  if (/(review|rating|google)/.test(l)) return <QuoteIcon size={size} />;
  if (/(builder|project|developer|society|units)/.test(l)) return <BuildingIcon size={size} />;
  return <PinIcon size={size} />;
}
