import { type EditablePlanInput, type PlanInputs } from "./model.ts";
import { useState } from "react";

const LAKH = 100_000;
const MONTHS_IN_YEAR = 12;

function monthlyInterestThresholdThousands(inputs: PlanInputs): number {
  const principal = inputs.propertyPriceLakh * LAKH * (1 - inputs.downPaymentPercent / 100);
  return Math.ceil((principal * inputs.loanRate / 100 / MONTHS_IN_YEAR) / 1_000);
}

type PlanAssumptionRailProps = {
  inputs: PlanInputs;
  extraEmisPerYear: number;
  loanFreeYear: number | null;
  onInputChange: (key: EditablePlanInput, value: number) => void;
  onExtraEmisChange: (count: number) => void;
  onReset: () => void;
};

type InputSpec = {
  key: EditablePlanInput;
  label: string;
  min: number;
  max?: number;
  prefix?: string;
  suffix: string;
  note: string;
};

function PlanInput({
  label,
  value,
  min,
  max,
  prefix,
  suffix,
  note,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max?: number;
  prefix?: string;
  suffix: string;
  note: string;
  onChange: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(value));
  const [editing, setEditing] = useState(false);

  function normalizeDraft(raw: string): string {
    const compact = raw.replace(/[^\d.]/g, "");
    const [head, ...tail] = compact.split(".");
    const integer = head.replace(/^0+(?=\d)/, "") || (compact.startsWith(".") ? "" : "0");
    const decimal = tail.length > 0 ? `.${tail.join("")}` : "";
    return `${integer}${decimal}`;
  }

  function parseInput(raw: string): number | null {
    if (!raw.trim()) return min;
    const next = Number(raw);
    if (!Number.isFinite(next)) return null;
    const bounded = Math.max(min, next);
    return max === undefined ? bounded : Math.min(max, bounded);
  }

  return (
    <label className="home-plan-inline-input">
      <span>{label}</span>
      <div>
        {prefix && <b>{prefix}</b>}
        <input
          type="text"
          inputMode="decimal"
          value={editing ? draft : String(value)}
          onChange={(event) => {
            const nextDraft = normalizeDraft(event.target.value);
            setDraft(nextDraft);
            const next = parseInput(nextDraft);
            if (next != null) onChange(next);
          }}
          onFocus={(event) => {
            setEditing(true);
            setDraft(String(value));
            event.currentTarget.select();
          }}
          onBlur={(event) => {
            const next = parseInput(event.currentTarget.value);
            const committed = next ?? value;
            setEditing(false);
            setDraft(String(committed));
            if (committed !== value) onChange(committed);
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
  loanFreeYear,
  onInputChange,
  onExtraEmisChange,
  onReset,
}: PlanAssumptionRailProps) {
  const principalThresholdThousands = monthlyInterestThresholdThousands(inputs);
  const emiNote = principalThresholdThousands > 0 && inputs.monthlyEmiThousands <= principalThresholdThousands
    ? `Principal starts above ₹${principalThresholdThousands.toLocaleString("en-IN")}K / mo`
    : loanFreeYear == null
      ? "Loan stays open at this EMI"
      : `Loan-free around year ${loanFreeYear}`;
  const buyPathInputs: InputSpec[] = [
    {
      key: "downPaymentPercent",
      label: "Down payment",
      min: 0,
      max: 100,
      suffix: "%",
      note: "Cash share of price",
    },
    {
      key: "loanRate",
      label: "Loan rate",
      min: 0,
      max: 15,
      suffix: "%",
      note: "Bank rate",
    },
  ];
  const rentPathInputs: InputSpec[] = [
    {
      key: "currentRentThousands",
      label: "Monthly rent",
      min: 0,
      prefix: "₹",
      suffix: "K / mo",
      note: "Monthly housing cost",
    },
    {
      key: "monthlySipThousands",
      label: "Monthly SIP",
      min: 0,
      prefix: "₹",
      suffix: "K / mo",
      note: "Rent-path investment",
    },
    {
      key: "equityReturn",
      label: "SIP return",
      min: 0,
      max: 20,
      suffix: "%",
      note: "Expected yearly gain",
    },
  ];

  return (
    <section className="home-plan-inline-studio" aria-label="Your monthly plan">
      <div className="home-plan-inline-studio__primary">
        <div className="home-plan-inline-studio__buy" role="group" aria-label="Buy plan">
          {buyPathInputs.map(({ key, ...input }) => (
            <PlanInput
              key={key}
              {...input}
              value={inputs[key]}
              onChange={(value) => onInputChange(key, value)}
            />
          ))}
          <PlanInput
            label="Monthly EMI"
            min={inputs.downPaymentPercent === 100 ? 0 : 1}
            prefix="₹"
            suffix="K / mo"
            note={emiNote}
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

        <div className="home-plan-inline-studio__rent" role="group" aria-label="Rent plan">
          {rentPathInputs.map(({ key, ...input }) => (
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
      </div>
    </section>
  );
}
