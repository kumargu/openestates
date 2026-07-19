import { useEffect, useRef } from "react";
import type { PropertyDetailResponse } from "../../lib/types.ts";
import { BUY_VS_RENT } from "./labels.ts";
import type { PlanControlField } from "./planFields.ts";
import { formatCurrency, type PlanInputs, type PlanProjection } from "./model.ts";

export type PlanControlSection = "financing" | "market" | "sources";
export type PlanPreset = "Base scenario" | "Cautious market" | "Strong growth" | "Custom";

type PlanControlsProps = {
  open: boolean;
  section: PlanControlSection;
  focusField: PlanControlField | null;
  preset: PlanPreset;
  inputs: PlanInputs;
  projection: PlanProjection;
  extraEmisPerYear: number;
  property: PropertyDetailResponse;
  onClose: () => void;
  onSectionChange: (section: PlanControlSection) => void;
  onPresetChange: (preset: Exclude<PlanPreset, "Custom">) => void;
  onInputChange: <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => void;
  onExtraEmisChange: (value: number) => void;
  onReset: () => void;
};

function ControlIcon({ name }: { name: PlanControlSection | "close" }) {
  const common = {
    width: 17,
    height: 17,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };
  if (name === "financing") return <svg {...common}><rect x="3" y="6" width="18" height="12" rx="2" /><path d="M7 10h3M7 14h6M17 10v4" /></svg>;
  if (name === "market") return <svg {...common}><path d="M4 19V9M10 19V5M16 19v-7M22 19H2" /><path d="m3 8 6-4 6 7 6-5" /></svg>;
  if (name === "sources") return <svg {...common}><path d="m12 3 7 3v5c0 4.6-2.8 8-7 10-4.2-2-7-5.4-7-10V6l7-3Z" /><path d="m9 12 2 2 4-5" /></svg>;
  return <svg {...common}><path d="m6 6 12 12M18 6 6 18" /></svg>;
}

function RangeControl({
  field,
  label,
  valueLabel,
  min,
  max,
  step,
  value,
  onChange,
  focused,
}: {
  field?: PlanControlField;
  label: string;
  valueLabel: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (value: number) => void;
  focused?: boolean;
}) {
  return (
    <label
      id={field ? `plan-field-${field}` : undefined}
      className={`home-plan-range-control ${focused ? "is-focused" : ""}`}
    >
      <span><span>{label}</span><strong>{valueLabel}</strong></span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(event) => onChange(Number(event.target.value))} />
    </label>
  );
}

function FinancingControls({
  inputs,
  projection,
  extraEmisPerYear,
  focusField,
  onInputChange,
  onExtraEmisChange,
}: Pick<PlanControlsProps, "inputs" | "projection" | "extraEmisPerYear" | "focusField" | "onInputChange" | "onExtraEmisChange">) {
  const nextDownPayment = Math.min(inputs.propertyPriceLakh * 0.8, inputs.downPaymentLakh + 10);
  const currentEmi = projection.monthlyEmi;
  const nextLoan = Math.max(0, inputs.propertyPriceLakh - nextDownPayment) * 100_000;
  const monthlyRate = inputs.loanRate / 100 / 12;
  const months = inputs.loanTenureYears * 12;
  const growth = (1 + monthlyRate) ** months;
  const nextEmi = monthlyRate === 0 ? nextLoan / months : nextLoan * monthlyRate * growth / (growth - 1);

  return (
    <>
      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Upfront</span>
        <h3>Down payment</h3>
        <p>More cash upfront lowers EMI, but leaves less in hand today.</p>
        <RangeControl
          field="downPaymentLakh"
          focused={focusField === "downPaymentLakh"}
          label="Down payment"
          valueLabel={`₹${inputs.downPaymentLakh.toFixed(0)}L`}
          min={10}
          max={Math.max(20, Math.floor(inputs.propertyPriceLakh * 0.8))}
          step={5}
          value={inputs.downPaymentLakh}
          onChange={(value) => onInputChange("downPaymentLakh", value)}
        />
        <div className="home-plan-control-insight">
          +₹10L down payment lowers EMI by {formatCurrency(Math.max(0, currentEmi - nextEmi))}/mo.
        </div>
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Loan</span>
        <RangeControl
          field="loanRate"
          focused={focusField === "loanRate"}
          label="Interest rate"
          valueLabel={`${inputs.loanRate.toFixed(1)}%`}
          min={6.5}
          max={11}
          step={0.1}
          value={inputs.loanRate}
          onChange={(value) => onInputChange("loanRate", value)}
        />
        <RangeControl label="Loan tenure" valueLabel={`${inputs.loanTenureYears} years`} min={10} max={30} step={5} value={inputs.loanTenureYears} onChange={(value) => onInputChange("loanTenureYears", value)} />
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Prepayment</span>
        <h3>{extraEmisPerYear} extra {extraEmisPerYear === 1 ? "EMI" : "EMIs"} per year</h3>
        <p>Your regular EMI stays the same. Extra payments go straight to principal.</p>
        <div className="home-plan-emi-options" aria-label="Extra EMIs each year">
          {[0, 1, 2, 3, 4, 6].map((count) => (
            <button type="button" key={count} className={extraEmisPerYear === count ? "is-active" : ""} onClick={() => onExtraEmisChange(count)}>{count}</button>
          ))}
        </div>
      </section>
    </>
  );
}

function MarketControls({
  inputs,
  focusField,
  onInputChange,
}: Pick<PlanControlsProps, "inputs" | "focusField" | "onInputChange">) {
  return (
    <>
      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Home</span>
        <h3>Price growth</h3>
        <p>This moves the buy path more than small monthly tweaks.</p>
        <RangeControl
          field="appreciation"
          focused={focusField === "appreciation"}
          label="Home appreciation"
          valueLabel={`${inputs.appreciation.toFixed(1)}%`}
          min={2}
          max={10}
          step={0.5}
          value={inputs.appreciation}
          onChange={(value) => onInputChange("appreciation", value)}
        />
        <RangeControl label="Rent inflation" valueLabel={`${inputs.rentInflation.toFixed(1)}%`} min={2} max={10} step={0.5} value={inputs.rentInflation} onChange={(value) => onInputChange("rentInflation", value)} />
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">If you rent</span>
        <h3>Mutual fund path</h3>
        <p>We invest the down payment and any monthly amount left after rent.</p>
        <RangeControl
          field="equityReturn"
          focused={focusField === "equityReturn"}
          label="Expected return"
          valueLabel={`${inputs.equityReturn.toFixed(1)}%`}
          min={6}
          max={14}
          step={0.5}
          value={inputs.equityReturn}
          onChange={(value) => onInputChange("equityReturn", value)}
        />
        <RangeControl label="Extra monthly SIP" valueLabel={`₹${inputs.monthlyExtraInvestmentThousands.toFixed(0)}K`} min={0} max={100} step={5} value={inputs.monthlyExtraInvestmentThousands} onChange={(value) => onInputChange("monthlyExtraInvestmentThousands", value)} />
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Timing</span>
        <RangeControl label="Buy after" valueLabel={inputs.purchaseYear === 0 ? "Now" : `${inputs.purchaseYear} years`} min={0} max={7} step={1} value={inputs.purchaseYear} onChange={(value) => onInputChange("purchaseYear", value)} />
        <RangeControl
          field="currentRentThousands"
          focused={focusField === "currentRentThousands"}
          label="Current rent"
          valueLabel={`₹${inputs.currentRentThousands.toFixed(0)}K / month`}
          min={15}
          max={150}
          step={5}
          value={inputs.currentRentThousands}
          onChange={(value) => onInputChange("currentRentThousands", value)}
        />
      </section>
    </>
  );
}

function SourcesPanel({ property }: Pick<PlanControlsProps, "property">) {
  const details = property.property;
  const area = property.area;
  const sourceFacts = property.source_panels?.flatMap((panel) => panel.items) ?? [];
  const freshestFact = sourceFacts[0];
  const evidence = [
    {
      label: "Listed price",
      value: formatCurrency(details.price, true),
      note: `${details.price_per_sqft.toLocaleString("en-IN")} / sqft`,
      confidence: property.confidence_score?.label ?? "From listing",
    },
    {
      label: "Area benchmark",
      value: area?.median_price_per_sqft ? `₹${area.median_price_per_sqft.toLocaleString("en-IN")} / sqft` : "Not available yet",
      note: area?.trend_summary ?? "We are still collecting area prices.",
      confidence: area ? "Area data" : "Pending",
    },
    {
      label: "RERA",
      value: property.rera?.registered ? "Registered" : "Not verified yet",
      note: property.rera?.registration_number ?? property.rera?.status ?? "No registration number on file.",
      confidence: property.rera?.registered ? "Verified" : "Check manually",
    },
    {
      label: "Latest fact",
      value: freshestFact?.value ?? "None yet",
      note: freshestFact ? `Sourced from ${freshestFact.source_type}` : "Assumptions below are editable until we have more data.",
      confidence: freshestFact?.learned_at ? `Updated ${new Date(freshestFact.learned_at).toLocaleDateString("en-IN")}` : "—",
    },
  ];

  return (
    <section className="home-plan-control-block home-plan-sources-block">
      <span className="home-plan-control-kicker">Sources</span>
      <h3>Where the numbers come from</h3>
      <p>Listing and area facts sit here. Loan and market assumptions are yours to change.</p>
      <div className="home-plan-evidence-list">
        {evidence.map((item) => (
          <div key={item.label}>
            <span>{item.label}</span>
            <strong>{item.value}</strong>
            <small>{item.note}</small>
            <em>{item.confidence}</em>
          </div>
        ))}
      </div>
    </section>
  );
}

export function PlanControls({
  open,
  section,
  focusField,
  preset,
  inputs,
  projection,
  extraEmisPerYear,
  property,
  onClose,
  onSectionChange,
  onPresetChange,
  onInputChange,
  onExtraEmisChange,
  onReset,
}: PlanControlsProps) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const sectionTitle = section === "financing" ? "Loan and down payment" : section === "market" ? "Market assumptions" : "Sources";

  useEffect(() => {
    if (!open || !focusField || !bodyRef.current) return;
    const target = bodyRef.current.querySelector<HTMLElement>(`#plan-field-${focusField}`);
    if (!target) return;
    target.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [open, focusField, section]);

  return (
    <>
      {open && <button type="button" className="home-plan-controls-backdrop" aria-label={BUY_VS_RENT.closeEditor} onClick={onClose} />}
      <aside className={`home-plan-controls ${open ? "is-open" : ""}`} aria-hidden={!open}>
        <header className="home-plan-controls-header">
          <div><span>{BUY_VS_RENT.assumptionsTitle}</span><h2>{sectionTitle}</h2></div>
          <button type="button" onClick={onClose} aria-label={BUY_VS_RENT.closeEditor}><ControlIcon name="close" /></button>
        </header>
        <section className="home-plan-preset-selector" aria-label="Scenario presets">
          <div>
            <span>Preset</span>
            <small>Base, cautious, or strong market — then tweak any slider.</small>
          </div>
          <div>
            {(["Base scenario", "Cautious market", "Strong growth"] as const).map((item) => (
              <button type="button" key={item} className={preset === item ? "is-active" : ""} onClick={() => onPresetChange(item)}>
                {item === "Base scenario" ? "Base" : item === "Cautious market" ? "Cautious" : "Strong"}
              </button>
            ))}
          </div>
          {preset === "Custom" && <em>Custom</em>}
        </section>
        <nav className="home-plan-controls-tabs" aria-label="Plan sections">
          {(["financing", "market", "sources"] as const).map((item) => (
            <button type="button" key={item} className={section === item ? "is-active" : ""} onClick={() => onSectionChange(item)}>
              <ControlIcon name={item} />
              <span>{item === "financing" ? "Loan" : item === "market" ? "Market" : "Sources"}</span>
            </button>
          ))}
        </nav>
        <div ref={bodyRef} className="home-plan-controls-body">
          {section === "financing" && <FinancingControls inputs={inputs} projection={projection} extraEmisPerYear={extraEmisPerYear} focusField={focusField} onInputChange={onInputChange} onExtraEmisChange={onExtraEmisChange} />}
          {section === "market" && <MarketControls inputs={inputs} focusField={focusField} onInputChange={onInputChange} />}
          {section === "sources" && <SourcesPanel property={property} />}
        </div>
        <footer className="home-plan-controls-footer">
          <button type="button" onClick={onReset}>Reset</button>
          <button type="button" className="home-plan-primary-action" onClick={onClose}>Done</button>
        </footer>
      </aside>
    </>
  );
}
