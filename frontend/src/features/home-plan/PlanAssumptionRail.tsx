import {
  formatLakhCurrency,
  formatMonthlyCurrency,
  type EditablePlanInput,
  type PlanInputs,
} from "./model.ts";
import { useState } from "react";
import type { RepaymentStrategy } from "./financeEngine.ts";

const LAKH = 100_000;
const MONTHS_IN_YEAR = 12;

function monthlyInterestThresholdThousands(inputs: PlanInputs): number {
  const principal = inputs.propertyPriceLakh * LAKH * (1 - inputs.downPaymentPercent / 100);
  return Math.ceil((principal * inputs.loanRate / 100 / MONTHS_IN_YEAR) / 1_000);
}

type PlanAssumptionRailProps = {
  inputs: PlanInputs;
  extraEmisPerYear: number;
  repaymentStrategy: RepaymentStrategy;
  showRepaymentObjective?: boolean;
  onInputChange: (key: EditablePlanInput, value: number) => void;
  onExtraEmisChange: (count: number) => void;
  onStrategyChange: (strategy: RepaymentStrategy) => void;
  onReset: () => void;
};

type InputSpec = {
  key: EditablePlanInput;
  label: string;
  min: number;
  max?: number;
  prefix?: string;
  suffix: string;
  note?: string;
};

function PlanInput({
  label,
  value,
  min,
  max,
  prefix,
  suffix,
  note,
  valueScale = 1,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max?: number;
  prefix?: string;
  suffix: string;
  note?: string;
  valueScale?: number;
  onChange: (value: number) => void;
}) {
  const displayValue = value / valueScale;
  const displayText = valueScale === 100 ? displayValue.toFixed(2) : String(displayValue);
  const [draft, setDraft] = useState(String(displayValue));
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
    const scaled = next * valueScale;
    const bounded = Math.max(min, scaled);
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
          value={editing ? draft : displayText}
          onChange={(event) => {
            const nextDraft = normalizeDraft(event.target.value);
            setDraft(nextDraft);
            const next = parseInput(nextDraft);
            if (next != null) onChange(next);
          }}
          onFocus={(event) => {
            setEditing(true);
            setDraft(String(displayValue));
            event.currentTarget.select();
          }}
          onBlur={(event) => {
            const next = parseInput(event.currentTarget.value);
            const committed = next ?? value;
            setEditing(false);
            setDraft(String(committed / valueScale));
            if (committed !== value) onChange(committed);
          }}
        />
        <b>{suffix}</b>
      </div>
      {note ? <small>{note}</small> : null}
    </label>
  );
}

export function PlanAssumptionRail({
  inputs,
  extraEmisPerYear,
  repaymentStrategy,
  showRepaymentObjective = true,
  onInputChange,
  onExtraEmisChange,
  onStrategyChange,
  onReset,
}: PlanAssumptionRailProps) {
  const principalThresholdThousands = monthlyInterestThresholdThousands(inputs);
  const emiNote = principalThresholdThousands > 0 && inputs.monthlyEmiThousands <= principalThresholdThousands
    ? `Must exceed ${formatMonthlyCurrency(principalThresholdThousands * 1_000)} to reduce principal`
    : "Before extra payments";
  const financingInputs: InputSpec[] = [
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
  return (
    <section
      className={`home-plan-inline-studio ${showRepaymentObjective ? "" : "is-rent-buy"}`}
      aria-label={showRepaymentObjective ? "Loan repayment assumptions" : "Buy assumptions"}
    >
      <div className="home-plan-inline-studio__primary">
        <div className="home-plan-inline-studio__buy" role="group" aria-label="Loan plan">
          {financingInputs.map(({ key, ...input }) => (
            <PlanInput
              key={key}
              {...input}
              note={showRepaymentObjective ? input.note : undefined}
              value={inputs[key]}
              onChange={(value) => onInputChange(key, value)}
            />
          ))}
          <PlanInput
            label="Monthly EMI"
            min={inputs.downPaymentPercent === 100 ? 0 : 1}
            prefix="₹"
            suffix="L/month"
            note={showRepaymentObjective ? emiNote : undefined}
            value={inputs.monthlyEmiThousands}
            valueScale={100}
            onChange={(value) => onInputChange("monthlyEmiThousands", value)}
          />
          <div className="home-plan-inline-studio__extra" role="group" aria-label="Extra payments each year">
            <span>Extra payments / year</span>
            <div>
              {[0, 1, 2, 3, 4, 6].map((count) => (
                <button
                  type="button"
                  key={count}
                  className={extraEmisPerYear === count ? "is-active" : undefined}
                  aria-pressed={extraEmisPerYear === count}
                  onClick={() => onExtraEmisChange(count)}
                >
                  {count}
                </button>
              ))}
            </div>
            <small>
              {showRepaymentObjective
                ? `Each stays at today's EMI: ${formatLakhCurrency(inputs.monthlyEmiThousands * 1_000)}`
                : "Each equals one monthly EMI"}
            </small>
          </div>
          {showRepaymentObjective ? (
            <div className="home-plan-repayment-use">
              <span>Apply extra payments to</span>
              <div role="group" aria-label="Apply extra payments to">
                <button
                  type="button"
                  className={repaymentStrategy === "finish_earlier" ? "is-active" : undefined}
                  aria-pressed={repaymentStrategy === "finish_earlier"}
                  onClick={() => onStrategyChange("finish_earlier")}
                >
                  Shorten tenure
                </button>
                <button
                  type="button"
                  className={repaymentStrategy === "lower_emi" ? "is-active" : undefined}
                  aria-pressed={repaymentStrategy === "lower_emi"}
                  onClick={() => onStrategyChange("lower_emi")}
                >
                  Lower EMI
                </button>
              </div>
            </div>
          ) : null}
        </div>

        {showRepaymentObjective ? (
          <div className="home-plan-inline-studio__repay-actions">
            <button type="button" className="home-plan-inline-studio__reset" onClick={onReset}>
              Reset plan
            </button>
          </div>
        ) : null}
      </div>
    </section>
  );
}

export function RentAssumptionRail({
  inputs,
  onInputChange,
  onReset,
}: {
  inputs: PlanInputs;
  onInputChange: (key: EditablePlanInput, value: number) => void;
  onReset: () => void;
}) {
  const rentInput = {
    label: "Monthly rent",
    min: 0,
    prefix: "₹",
    suffix: "L/month",
  };
  const returnInput = {
    label: "SIP return",
    min: 0,
    max: 20,
    suffix: "%",
    note: "Expected yearly gain",
  };
  const selectedSipMultiple = [1, 2, 3].find((multiple) => (
    Math.abs(inputs.monthlySipThousands - inputs.monthlyEmiThousands * multiple) < 0.01
  ));

  return (
    <section className="home-plan-rent-controls" aria-label="Rent and investment assumptions">
      <PlanInput
        {...rentInput}
        value={inputs.currentRentThousands}
        valueScale={100}
        onChange={(value) => onInputChange("currentRentThousands", value)}
      />
      <div className="home-plan-sip-multiple">
        <span>Monthly SIP</span>
        <div role="group" aria-label="Monthly SIP as a multiple of EMI">
          {[1, 2, 3].map((multiple) => (
            <button
              type="button"
              key={multiple}
              className={selectedSipMultiple === multiple ? "is-active" : undefined}
              aria-pressed={selectedSipMultiple === multiple}
              onClick={() => onInputChange(
                "monthlySipThousands",
                inputs.monthlyEmiThousands * multiple,
              )}
            >
              {multiple}× EMI
            </button>
          ))}
        </div>
      </div>
      <div className="home-plan-rent-actions">
        <details className="home-plan-rent-assumptions">
          <summary>Assumptions</summary>
          <div className="home-plan-rent-assumptions__body">
            <PlanInput
              {...returnInput}
              value={inputs.equityReturn}
              onChange={(value) => onInputChange("equityReturn", value)}
            />
            <dl>
              <div>
                <dt>Home growth</dt>
                <dd>{inputs.assumptions.homeAppreciationRate}% / year</dd>
              </div>
              <div>
                <dt>Rent growth</dt>
                <dd>{inputs.assumptions.rentInflationRate}% / year</dd>
              </div>
            </dl>
          </div>
        </details>
        <button type="button" onClick={onReset}>Reset plan</button>
      </div>
    </section>
  );
}
