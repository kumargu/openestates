import type { ReraInfo } from "./types.ts";

export type ReraFactTone = "default" | "positive" | "watch";

export type ReraFactRow = {
  label: string;
  value: string;
  tone?: ReraFactTone;
  code?: boolean;
};

export type ReraFactGroup = {
  id: "registration" | "schedule" | "scale" | "checks";
  label: string;
  rows: ReraFactRow[];
};

function knownText(value?: string): string | null {
  if (!value) return null;
  const normalized = value.trim();
  if (!normalized) return null;
  if (["unknown", "not specified", "n/a", "na", "none"].includes(normalized.toLowerCase())) {
    return null;
  }
  return normalized;
}

function formatDate(value?: string): string | null {
  const known = knownText(value);
  if (!known) return null;
  const date = new Date(known);
  if (Number.isNaN(date.getTime())) return known;
  return new Intl.DateTimeFormat("en-IN", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(date);
}

function formatStatus(value?: string): string | null {
  const normalized = knownText(value);
  if (!normalized) return null;
  if (normalized === normalized.toUpperCase()) {
    const lower = normalized.toLowerCase();
    return `${lower.charAt(0).toUpperCase()}${lower.slice(1)}`;
  }
  return normalized;
}

function statusRow(rera: ReraInfo): ReraFactRow {
  const status = formatStatus(rera.status);
  if (!status) {
    return {
      label: "Status",
      value: rera.registered ? "Registered" : "Registration not confirmed",
      tone: rera.registered ? "positive" : "watch",
    };
  }
  const reassuring = new Set(["approved", "active", "registered", "registration approved"])
    .has(status.toLowerCase());
  return {
    label: "Status",
    value: status,
    tone: rera.registered && reassuring ? "positive" : "watch",
  };
}

function compactNumber(value: number): string {
  return value.toLocaleString("en-IN", { maximumFractionDigits: 1 });
}

function rows(values: Array<ReraFactRow | null>): ReraFactRow[] {
  return values.filter((value): value is ReraFactRow => value != null);
}

export function reraFactGroups(rera: ReraInfo): ReraFactGroup[] {
  const start = formatDate(rera.start_date);
  const target = formatDate(rera.completion_date);
  const registrationNumber = knownText(rera.registration_number);
  const escrowBank = knownText(rera.escrow_bank);
  const originalTarget = rera.original_completion_date !== rera.completion_date
    ? formatDate(rera.original_completion_date)
    : null;
  const siteArea = rera.total_land_area_acres != null
    ? `${compactNumber(rera.total_land_area_acres)} acres`
    : rera.total_land_area_sqm != null
      ? `${compactNumber(rera.total_land_area_sqm)} sqm`
      : null;
  const complaints = rera.complaints_count != null
    ? [
        `${compactNumber(rera.complaints_count)} filed`,
        rera.complaints_resolved_pct != null
          ? `${compactNumber(rera.complaints_resolved_pct)}% resolved`
          : null,
      ].filter(Boolean).join(" · ")
    : null;

  const groups: ReraFactGroup[] = [
    {
      id: "registration",
      label: "Registration",
      rows: rows([
        statusRow(rera),
        registrationNumber
          ? { label: "RERA number", value: registrationNumber, code: true }
          : null,
        escrowBank
          ? { label: "Escrow bank", value: escrowBank }
          : null,
      ]),
    },
    {
      id: "schedule",
      label: "Schedule",
      rows: rows([
        start ? { label: "Declared start", value: start } : null,
        target ? { label: "Current target", value: target } : null,
        originalTarget ? { label: "Original target", value: originalTarget } : null,
        rera.delay_months != null && rera.delay_months > 0
          ? { label: "Recorded delay", value: `${rera.delay_months} months`, tone: "watch" }
          : null,
      ]),
    },
    {
      id: "scale",
      label: "Project scale",
      rows: rows([
        siteArea ? { label: "Site area", value: siteArea } : null,
        rera.total_units != null
          ? { label: "Homes", value: compactNumber(rera.total_units) }
          : null,
        rera.open_area_pct != null
          ? { label: "Open area", value: `${compactNumber(rera.open_area_pct)}%` }
          : null,
        rera.units_per_acre != null
          ? { label: "Density", value: `${compactNumber(rera.units_per_acre)} homes/acre` }
          : null,
      ]),
    },
    {
      id: "checks",
      label: "Buyer checks",
      rows: rows([
        complaints
          ? {
              label: "Complaints",
              value: complaints,
              tone: rera.complaints_count && rera.complaints_count > 0 ? "watch" : "positive",
            }
          : null,
        rera.land_litigation != null
          ? {
              label: "Land litigation",
              value: rera.land_litigation ? "Recorded" : "None recorded",
              tone: rera.land_litigation ? "watch" : "positive",
            }
          : null,
        rera.builder_revocations != null
          ? {
              label: "Builder revocations",
              value: rera.builder_revocations > 0
                ? `${rera.builder_revocations} recorded`
                : "None recorded",
              tone: rera.builder_revocations > 0 ? "watch" : "positive",
            }
          : null,
        rera.builder_total_projects != null
          ? { label: "Builder record", value: `${rera.builder_total_projects} RERA projects` }
          : null,
        rera.has_borrowing != null
          ? {
              label: "Project borrowing",
              value: rera.has_borrowing ? "Reported" : "None reported",
              tone: rera.has_borrowing ? "watch" : "positive",
            }
          : null,
        rera.has_mortgage != null
          ? {
              label: "Project mortgage",
              value: rera.has_mortgage ? "Reported" : "None reported",
              tone: rera.has_mortgage ? "watch" : "positive",
            }
          : null,
      ]),
    },
  ];
  return groups.filter((group) => group.rows.length > 0);
}

export function reraFactCount(rera: ReraInfo): number {
  return reraFactGroups(rera).reduce((total, group) => total + group.rows.length, 0);
}
