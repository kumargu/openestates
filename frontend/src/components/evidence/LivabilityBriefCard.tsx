import type { LivabilityBrief } from "../../lib/types.ts";

type Props = {
  brief: LivabilityBrief;
};

export function LivabilityBriefCard({ brief }: Props) {
  const summary = brief.summary_paragraph?.trim();
  if (!summary) return null;

  return (
    <section className="livability-brief" aria-label="Livability brief">
      <div className="livability-brief__header">
        <div className="property-section-heading">
          <span>Neighborhood</span>
          <h2>Livability brief</h2>
        </div>
      </div>

      {brief.lifecycle_flag && (
        <p className="livability-brief__flag">{formatLifecycleFlag(brief.lifecycle_flag)}</p>
      )}

      <p className="livability-brief__summary">{summary}</p>
    </section>
  );
}

function formatLifecycleFlag(flag: string): string {
  switch (flag) {
    case "livability-first":
      return "Livability-first society";
    case "ready-to-move":
      return "Ready to move";
    case "understand-before-you-buy":
      return "Under construction";
    default:
      return flag.replaceAll("-", " ");
  }
}
