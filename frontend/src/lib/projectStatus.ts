export type BuyerProjectStatusKey =
  | "ready_to_move"
  | "under_construction"
  | "new_launch"
  | "delayed"
  | "upcoming";

export type BuyerProjectStatus = {
  key: BuyerProjectStatusKey;
  label: string;
};

const STATUS_LABELS: Record<BuyerProjectStatusKey, string> = {
  ready_to_move: "Ready to move",
  under_construction: "Under construction",
  new_launch: "New launch",
  delayed: "Delayed",
  upcoming: "Upcoming",
};

function normalizeStatus(value: string): string {
  const normalized = value.toLowerCase().replace(/[_\s-]+/g, "_");
  if (normalized === "ready") return "ready_to_move";
  if (normalized.includes("construction")) return "under_construction";
  if (normalized.includes("new_launch")) return "new_launch";
  if (normalized.includes("delay")) return "delayed";
  if (normalized.includes("upcoming")) return "upcoming";
  return normalized;
}

function isBuyerProjectStatusKey(value: string): value is BuyerProjectStatusKey {
  return Object.hasOwn(STATUS_LABELS, value);
}

export function resolveBuyerProjectStatus(input: {
  status?: string;
  displayText?: string;
  possessionStatus?: string;
}): BuyerProjectStatus | null {
  const key = input.status
    || (input.possessionStatus ? normalizeStatus(input.possessionStatus) : null);
  if (!key) return null;
  if (!isBuyerProjectStatusKey(key)) return null;
  return {
    key,
    label: input.displayText || STATUS_LABELS[key],
  };
}
