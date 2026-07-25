/**
 * Soft Notion-inspired product icons.
 * Pastel chip + simple glyph so meaning stays obvious at chip size.
 */
import type { ReactNode } from "react";

export type SoftTone =
  | "clay"
  | "cool"
  | "teal"
  | "sage"
  | "amber"
  | "lilac"
  | "rose"
  | "slate";

type SoftIconProps = {
  size?: number;
  tone?: SoftTone;
  label?: string;
};

const TONE_FILL: Record<SoftTone, string> = {
  clay: "#F3D6C8",
  cool: "#D7E2F2",
  teal: "#D2E8E6",
  sage: "#D9E8D6",
  amber: "#F3E4C2",
  lilac: "#E5DCF2",
  rose: "#F0D7DE",
  slate: "#E4E2DF",
};

function SoftBadge({
  size = 18,
  tone = "slate",
  label,
  children,
}: SoftIconProps & { children: ReactNode }) {
  const px = `${size}px`;
  return (
    <span
      className={`soft-icon soft-icon--${tone}`}
      style={{
        width: px,
        height: px,
        background: TONE_FILL[tone],
      }}
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? "img" : undefined}
    >
      <svg
        width={Math.round(size * 0.62)}
        height={Math.round(size * 0.62)}
        viewBox="0 0 24 24"
        fill="none"
        stroke="#2A2623"
        strokeWidth="1.9"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {children}
      </svg>
    </span>
  );
}

/** Nearby essentials — soft spark for the mixed nearby set. */
export function SoftEssentialsIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "clay"}>
      <path d="M12 4.2 13.2 9.4 18.4 10.6 13.2 11.8 12 17 10.8 11.8 5.6 10.6 10.8 9.4z" />
      <circle cx="18.2" cy="5.8" r="1" fill="#2A2623" stroke="none" />
      <circle cx="5.8" cy="16.8" r="0.85" fill="#2A2623" stroke="none" />
    </SoftBadge>
  );
}

/** Metro — soft train car. */
export function SoftMetroIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "cool"}>
      <rect x="6" y="4.5" width="12" height="11" rx="3" />
      <path d="M6 11h12" />
      <circle cx="9.2" cy="14" r="0.7" fill="#2A2623" stroke="none" />
      <circle cx="14.8" cy="14" r="0.7" fill="#2A2623" stroke="none" />
      <path d="M8.2 19.2l1.8-2.2M15.8 19.2l-1.8-2.2" />
    </SoftBadge>
  );
}

/** Schools — soft graduation cap. */
export function SoftSchoolIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "amber"}>
      <path d="M3.8 10.2 12 6l8.2 4.2L12 14.4 3.8 10.2z" />
      <path d="M7.2 12v3.4c0 1.2 2.2 2.3 4.8 2.3s4.8-1.1 4.8-2.3V12" />
      <path d="M19.6 10.8v4.2" />
    </SoftBadge>
  );
}

/** Hospitals — soft clinic with plus. */
export function SoftHospitalIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "rose"}>
      <rect x="5" y="4.5" width="14" height="15" rx="3" />
      <path d="M12 8.2v7.2M8.4 11.8h7.2" />
    </SoftBadge>
  );
}

/** Tech parks — soft building block. */
export function SoftTechIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "lilac"}>
      <rect x="5.5" y="4" width="13" height="16" rx="2.2" />
      <path d="M9 8h2M13 8h2M9 12h2M13 12h2M9 16h6" />
    </SoftBadge>
  );
}

/** Water — soft droplet. */
export function SoftWaterIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "teal"}>
      <path d="M12 4.2c2.8 3.4 5.2 6.2 5.2 9.1a5.2 5.2 0 0 1-10.4 0c0-2.9 2.4-5.7 5.2-9.1z" />
    </SoftBadge>
  );
}

/** Usable space — soft floor plan. */
export function SoftSpaceIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "cool"}>
      <rect x="4.5" y="5" width="15" height="14" rx="2.2" />
      <path d="M4.5 12h15M12 5v14" />
    </SoftBadge>
  );
}

/** Land / acres — soft plot with sprout. */
export function SoftLandIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "sage"}>
      <path d="M4.8 16.8 9.8 9.5l9.4 2.2-5 7.1z" />
      <path d="M11.2 11.2V7.4" />
      <path d="M11.2 8.8c-1.5-.2-2.4-1-2.5-2.2 1.6 0 2.4.8 2.5 2.2z" />
      <path d="M11.2 8.8c.2-1.5 1.1-2.2 2.6-2.1-.2 1.5-1.1 2.2-2.6 2.1z" />
    </SoftBadge>
  );
}

/** Home state — soft house. */
export function SoftHomeStateIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "amber"}>
      <path d="M4.8 11.2 12 5.2l7.2 6V19a1.6 1.6 0 0 1-1.6 1.6H6.4A1.6 1.6 0 0 1 4.8 19z" />
      <path d="M10 20.6v-5.2h4v5.2" />
    </SoftBadge>
  );
}

/** Builder — soft hard-hat building cue. */
export function SoftBuilderIcon(props: SoftIconProps) {
  return (
    <SoftBadge {...props} tone={props.tone ?? "clay"}>
      <path d="M7 11.5a5 5 0 0 1 10 0" />
      <path d="M5.5 11.5h13" />
      <path d="M8 11.5v8.2M16 11.5v8.2M8 19.7h8" />
    </SoftBadge>
  );
}

export function SoftNearbyIcon({
  kind,
  size = 32,
}: {
  kind: "essentials" | "metro" | "schools" | "hospitals" | "tech" | "water" | string;
  size?: number;
}) {
  switch (kind) {
    case "essentials":
      return <SoftEssentialsIcon size={size} />;
    case "metro":
      return <SoftMetroIcon size={size} />;
    case "schools":
      return <SoftSchoolIcon size={size} />;
    case "hospitals":
      return <SoftHospitalIcon size={size} />;
    case "tech":
      return <SoftTechIcon size={size} />;
    case "water":
      return <SoftWaterIcon size={size} />;
    default:
      return <SoftEssentialsIcon size={size} />;
  }
}

export function SoftComparableIcon({
  id,
  size = 16,
}: {
  id: "space" | "land" | "openSpace" | "homeState" | "builder" | string;
  size?: number;
}) {
  switch (id) {
    case "space":
      return <SoftSpaceIcon size={size} />;
    case "land":
    case "openSpace":
      return <SoftLandIcon size={size} />;
    case "homeState":
      return <SoftHomeStateIcon size={size} />;
    case "builder":
      return <SoftBuilderIcon size={size} />;
    default:
      return <SoftEssentialsIcon size={size} />;
  }
}
