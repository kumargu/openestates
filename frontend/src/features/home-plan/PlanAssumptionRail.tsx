import type { PlanInputs } from "./model.ts";

type PlanAssumptionRailProps = {
  inputs: PlanInputs;
  onInputChange: <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => void;
  onReset: () => void;
};

type DialSpec = {
  key: keyof PlanInputs;
  label: string;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
};

function clamp(value: number, min: number, max: number, step: number): number {
  const snapped = Math.round(value / step) * step;
  return Math.min(max, Math.max(min, Number(snapped.toFixed(4))));
}

function ValueDial({
  label,
  display,
  atMin,
  atMax,
  onDecrease,
  onIncrease,
}: {
  label: string;
  display: string;
  atMin: boolean;
  atMax: boolean;
  onDecrease: () => void;
  onIncrease: () => void;
}) {
  return (
    <div className="home-plan-dial">
      <span className="home-plan-dial__label">{label}</span>
      <div className="home-plan-dial__control">
        <button type="button" className="home-plan-dial__step" onClick={onDecrease} disabled={atMin} aria-label={`Decrease ${label}`}>
          −
        </button>
        <span className="home-plan-dial__value">{display}</span>
        <button type="button" className="home-plan-dial__step" onClick={onIncrease} disabled={atMax} aria-label={`Increase ${label}`}>
          +
        </button>
      </div>
    </div>
  );
}

export function PlanAssumptionRail({ inputs, onInputChange, onReset }: PlanAssumptionRailProps) {
  const maxDown = Math.max(20, Math.floor(inputs.propertyPriceLakh * 0.8));

  const dials: DialSpec[] = [
    {
      key: "downPaymentLakh",
      label: "Down",
      min: 10,
      max: maxDown,
      step: 5,
      format: (v) => `₹${v.toFixed(0)}L`,
    },
    {
      key: "loanRate",
      label: "Loan",
      min: 6.5,
      max: 11,
      step: 0.1,
      format: (v) => `${v.toFixed(1)}%`,
    },
    {
      key: "appreciation",
      label: "Growth",
      min: 2,
      max: 10,
      step: 0.5,
      format: (v) => `${v.toFixed(1)}%`,
    },
    {
      key: "equityReturn",
      label: "Funds",
      min: 6,
      max: 14,
      step: 0.5,
      format: (v) => `${v.toFixed(1)}%`,
    },
    {
      key: "currentRentThousands",
      label: "Rent",
      min: 15,
      max: 150,
      step: 5,
      format: (v) => `₹${v.toFixed(0)}K`,
    },
    {
      key: "rentInflation",
      label: "Rent rise",
      min: 2,
      max: 12,
      step: 0.5,
      format: (v) => `${v.toFixed(1)}%/yr`,
    },
  ];

  return (
    <aside className="home-plan-assumption-rail" aria-label="Plan assumptions">
      <div className="home-plan-assumption-rail__dials">
        {dials.map((dial) => {
          const value = inputs[dial.key] as number;
          return (
            <ValueDial
              key={dial.key}
              label={dial.label}
              display={dial.format(value)}
              atMin={value <= dial.min}
              atMax={value >= dial.max}
              onDecrease={() => onInputChange(dial.key, clamp(value - dial.step, dial.min, dial.max, dial.step) as PlanInputs[typeof dial.key])}
              onIncrease={() => onInputChange(dial.key, clamp(value + dial.step, dial.min, dial.max, dial.step) as PlanInputs[typeof dial.key])}
            />
          );
        })}
      </div>
      <button type="button" className="home-plan-assumption-rail__reset" onClick={onReset}>
        Reset
      </button>
    </aside>
  );
}
