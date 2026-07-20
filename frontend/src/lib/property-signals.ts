/** Normalize buyer-facing status copy for duplicate checks. */
function normalizeSignal(value: string): string {
  return value.toLowerCase().replace(/[_\s·.,-]+/g, " ").trim();
}

/** True when home_state_display repeats project/possession status already shown elsewhere. */
export function isRedundantHomeState(
  homeState: string,
  projectStatusDisplay?: string,
  possessionStatus?: string,
): boolean {
  const home = normalizeSignal(homeState);
  const project = projectStatusDisplay ? normalizeSignal(projectStatusDisplay) : "";
  const possession = possessionStatus ? normalizeSignal(possessionStatus) : "";

  if (project && (home === project || home.startsWith(project) || project.startsWith(home))) {
    return true;
  }
  if (possession && home === possession) {
    return true;
  }
  return false;
}

/** Evidence kinds promoted to a dedicated primary surface on property detail. */
export function detailEvidenceExcludeKinds(showApproachTrail: boolean): string[] {
  return showApproachTrail ? ["approach_road"] : [];
}
