import { type PlanInputs } from "./model.ts";

type PlanAssumptionRailProps = {
  inputs: PlanInputs;
  extraEmisPerYear: number;
  onInputChange: <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => void;
  onExtraEmisChange: (count: number) => void;
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
  extraEmisPerYear,
  onInputChange,
  onExtraEmisChange,
  onReset,
}: PlanAssumptionRailProps) {
  const rentPathInputs: InputSpec[] = [
    {
      key: "currentRentThousands",
      label: "Monthly rent",
      min: 0,
      max: 150,
      step: 5,
      prefix: "₹",
      suffix: "K / mo",
      note: "Cash out while renting",
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
      note: "Bank rate",
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
        <div className="home-plan-inline-studio__buy">
          <PlanInput
            label="Monthly EMI"
            min={0}
            step={5}
            prefix="₹"
            suffix="K / mo"
            note="Your buy plan"
            value={inputs.monthlyEmiThousands}
            onChange={(value) => onInputChange("monthlyEmiThousands", value)}
          />
          <div className="home-plan-inline-studio__extra" role="group" aria-label="Extra EMIs each year">
            <span>Extra EMIs / year</span>
            <div>
              {[0, 1, 2, 3, 4, 6].map((count) => (
                <button
                  type="button"
                  key={count}
                  className={extraEmisPerYear === count ? "is-active" : ""}
                  aria-pressed={extraEmisPerYear === count}
                  onClick={() => onExtraEmisChange(count)}
                >
                  {count}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="home-plan-inline-studio__divider" aria-hidden="true" />

        <div className="home-plan-inline-studio__rent">
          {rentPathInputs.map(({ key, ...input }) => (
            <PlanInput
              key={key}
              {...input}
              value={inputs[key]}
              onChange={(value) => onInputChange(key, value)}
            />
          ))}
        </div>
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
        <button type="button" className="home-plan-inline-studio__reset" onClick={onReset}>
          Reset
        </button>
      </div>
    </section>
  );
}
