import type { PlanControlSection } from "./PlanControls.tsx";
import type { PlanInputs } from "./model.ts";

type AssumptionStripProps = {
  inputs: PlanInputs;
  onEdit: (section: PlanControlSection) => void;
};

export function AssumptionStrip({ inputs, onEdit }: AssumptionStripProps) {
  const chips: Array<{ label: string; value: string; section: PlanControlSection }> = [
    { label: "Down", value: `₹${inputs.downPaymentLakh.toFixed(0)}L`, section: "financing" },
    { label: "Rate", value: `${inputs.loanRate.toFixed(1)}%`, section: "financing" },
    { label: "Growth", value: `${inputs.appreciation.toFixed(1)}%`, section: "market" },
    { label: "Funds", value: `${inputs.equityReturn.toFixed(1)}%`, section: "market" },
    { label: "Rent", value: `₹${inputs.currentRentThousands.toFixed(0)}K`, section: "market" },
  ];

  return (
    <footer className="home-plan-instruments" aria-label="Plan assumptions">
      <div className="home-plan-instruments__chips">
        {chips.map((chip) => (
          <button
            type="button"
            key={chip.label}
            className="home-plan-instruments__chip"
            onClick={() => onEdit(chip.section)}
          >
            <small>{chip.label}</small>
            <strong>{chip.value}</strong>
          </button>
        ))}
      </div>
      <button type="button" className="home-plan-instruments__edit" onClick={() => onEdit("financing")}>
        Adjust assumptions
      </button>
    </footer>
  );
}
