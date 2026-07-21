import type { EntityContextResponse } from "../../lib/types.ts";

type Props = {
  context: EntityContextResponse;
};

export function EntityContextCard({ context }: Props) {
  if (!context.summary_paragraph.trim()) return null;

  return (
    <section className="property-evidence-section entity-context-card" aria-label="Neighborhood context">
      <div className="property-section-heading">
        <span>Neighborhood</span>
      </div>
      <p className="entity-context-card__summary">{context.summary_paragraph}</p>
    </section>
  );
}
