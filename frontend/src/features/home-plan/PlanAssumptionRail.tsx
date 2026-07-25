import {
  type PlanInputs,
} from "./model.ts";

type PlanAssumptionRailProps = {
  inputs: PlanInputs;
  onInputChange: <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => void;
  onReset: () => void;
};

type InputSpec = {
  key: "monthlyEmiThousands" | "currentRentThousands" | "monthlySipThousands" | "loanRate" | "equityReturn";
  label: string;
  min: number;
  max?: number;
  step: number;
  prefix?: string;
  suffix: string;
  note: string;
};

function PlanInput({
  label,
  value,
  min,
  max,
  step,
  prefix,
  suffix,
  note,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max?: number;
  step: number;
  prefix?: string;
  suffix: string;
  note: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="home-plan-inline-input">
      <span>{label}</span>
      <div>
        {prefix && <b>{prefix}</b>}
        <input
          type="number"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(event) => {
            const next = Number(event.target.value);
            if (Number.isFinite(next)) {
              const bounded = Math.max(min, next);
              onChange(max === undefined ? bounded : Math.min(max, bounded));
            }
          }}
        />
        <b>{suffix}</b>
      </div>
      <small>{note}</small>
    </label>
  );
}

export function PlanAssumptionRail({
  inputs,
  onInputChange,
  onReset,
}: PlanAssumptionRailProps) {
  const primaryInputs: InputSpec[] = [
    {
      key: "monthlyEmiThousands",
      label: "Monthly EMI",
      min: 0,
      step: 5,
      prefix: "₹",
      suffix: "K / mo",
      note: "Your buy plan",
    },
    {
      key: "currentRentThousands",
      label: "Monthly rent",
      min: 0,
      max: 150,
      step: 5,
      prefix: "₹",
      suffix: "K / mo",
      note: "Your rent today",
    },
    {
      key: "monthlySipThousands",
      label: "Monthly SIP",
      min: 0,
      max: 250,
      step: 5,
      prefix: "₹",
      suffix: "K / mo",
      note: "Your rent-path investment",
    },
  ];

  const rateInputs: InputSpec[] = [
    {
      key: "loanRate",
      label: "Loan rate",
      min: 0,
      max: 15,
      step: 0.1,
      suffix: "%",
      note: "Fixed for this estimate",
    },
    {
      key: "equityReturn",
      label: "SIP return",
      min: 0,
      max: 20,
      step: 0.5,
      suffix: "%",
      note: "Expected yearly gain",
    },
  ];

  return (
    <section className="home-plan-inline-studio" aria-label="Your monthly plan">
      <div className="home-plan-inline-studio__primary">
        {primaryInputs.map(({ key, ...input }) => (
          <PlanInput
            key={key}
            {...input}
            value={inputs[key]}
            onChange={(value) => onInputChange(key, value)}
          />
        ))}
      </div>
      <div className="home-plan-inline-studio__rates">
        {rateInputs.map(({ key, ...input }) => (
          <PlanInput
            key={key}
            {...input}
            value={inputs[key]}
            onChange={(value) => onInputChange(key, value)}
          />
        ))}
        <button type="button" onClick={onReset}>Reset</button>
      </div>
    </section>
  );
}
