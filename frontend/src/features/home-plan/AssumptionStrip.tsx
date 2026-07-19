import type { PlanControlField } from "./planFields.ts";
import { assumptionChips } from "./planFields.ts";
import type { PlanControlSection } from "./PlanControls.tsx";
import type { PlanInputs } from "./model.ts";
import { BUY_VS_RENT } from "./labels.ts";

type AssumptionStripProps = {
  inputs: PlanInputs;
  activeField: PlanControlField | null;
  onEdit: (section: PlanControlSection, field: PlanControlField) => void;
};

export function AssumptionStrip({ inputs, activeField, onEdit }: AssumptionStripProps) {
  const chips = assumptionChips(inputs);

  return (
    <footer className="home-plan-instruments" aria-label="Buy vs rent assumptions">
      <p className="home-plan-instruments__hint">Tap a number to edit it</p>
      <div className="home-plan-instruments__chips">
        {chips.map((chip) => (
          <button
            type="button"
            key={chip.field}
            className={`home-plan-instruments__chip ${activeField === chip.field ? "is-active" : ""}`}
            onClick={() => onEdit(chip.section, chip.field)}
            aria-label={`Edit ${chip.label}: ${chip.value}`}
          >
            <small>{chip.label}</small>
            <strong>{chip.value}</strong>
          </button>
        ))}
      </div>
      <button type="button" className="home-plan-instruments__edit" onClick={() => onEdit("financing", "downPaymentLakh")}>
        {BUY_VS_RENT.editAssumptions}
      </button>
    </footer>
  );
}
