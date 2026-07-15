import type { PropertyDetailResponse } from "../../lib/types.ts";
import { formatCurrency, type PlanInputs, type PlanProjection } from "./model.ts";

export type PlanControlSection = "financing" | "market" | "sources";
export type PlanPreset = "Base scenario" | "Cautious market" | "Strong growth" | "Custom";

type PlanControlsProps = {
  open: boolean;
  section: PlanControlSection;
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
  label,
  valueLabel,
  min,
  max,
  step,
  value,
  onChange,
}: {
  label: string;
  valueLabel: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="home-plan-range-control">
      <span><span>{label}</span><strong>{valueLabel}</strong></span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(event) => onChange(Number(event.target.value))} />
    </label>
  );
}

function FinancingControls({
  inputs,
  projection,
  extraEmisPerYear,
  onInputChange,
  onExtraEmisChange,
}: Pick<PlanControlsProps, "inputs" | "projection" | "extraEmisPerYear" | "onInputChange" | "onExtraEmisChange">) {
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
        <span className="home-plan-control-kicker">Upfront cash</span>
        <h3>Down payment</h3>
        <p>Change the cash committed and see EMI, liquidity, and long-term outcome move together.</p>
        <RangeControl
          label="Cash committed"
          valueLabel={`₹${inputs.downPaymentLakh.toFixed(0)}L`}
          min={10}
          max={Math.max(20, Math.floor(inputs.propertyPriceLakh * 0.8))}
          step={5}
          value={inputs.downPaymentLakh}
          onChange={(value) => onInputChange("downPaymentLakh", value)}
        />
        <div className="home-plan-control-insight">
          Adding ₹10L lowers EMI by {formatCurrency(Math.max(0, currentEmi - nextEmi))}, but leaves ₹10L less liquid today.
        </div>
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Loan terms</span>
        <RangeControl label="Interest rate" valueLabel={`${inputs.loanRate.toFixed(1)}%`} min={6.5} max={11} step={0.1} value={inputs.loanRate} onChange={(value) => onInputChange("loanRate", value)} />
        <RangeControl label="Loan tenure" valueLabel={`${inputs.loanTenureYears} years`} min={10} max={30} step={5} value={inputs.loanTenureYears} onChange={(value) => onInputChange("loanTenureYears", value)} />
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Prepayment</span>
        <h3>{extraEmisPerYear} extra {extraEmisPerYear === 1 ? "EMI" : "EMIs"} each year</h3>
        <p>Keep the regular EMI unchanged and use annual prepayments to shorten the loan journey.</p>
        <div className="home-plan-emi-options" aria-label="Extra EMIs each year">
          {[0, 1, 2, 3, 4, 6].map((count) => (
            <button type="button" key={count} className={extraEmisPerYear === count ? "is-active" : ""} onClick={() => onExtraEmisChange(count)}>{count}</button>
          ))}
        </div>
      </section>
    </>
  );
}

function MarketControls({ inputs, onInputChange }: Pick<PlanControlsProps, "inputs" | "onInputChange">) {
  return (
    <>
      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Home market</span>
        <h3>Property assumptions</h3>
        <p>Use a cautious range. Appreciation changes the ownership outcome more than most monthly tweaks.</p>
        <RangeControl label="Home appreciation" valueLabel={`${inputs.appreciation.toFixed(1)}%`} min={2} max={10} step={0.5} value={inputs.appreciation} onChange={(value) => onInputChange("appreciation", value)} />
        <RangeControl label="Rent inflation" valueLabel={`${inputs.rentInflation.toFixed(1)}%`} min={2} max={10} step={0.5} value={inputs.rentInflation} onChange={(value) => onInputChange("rentInflation", value)} />
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Alternative investment</span>
        <h3>Mutual-fund assumptions</h3>
        <p>The rent scenario invests the down payment and the monthly EMI difference.</p>
        <RangeControl label="Expected return" valueLabel={`${inputs.equityReturn.toFixed(1)}%`} min={6} max={14} step={0.5} value={inputs.equityReturn} onChange={(value) => onInputChange("equityReturn", value)} />
        <RangeControl label="Extra monthly SIP" valueLabel={`₹${inputs.monthlyExtraInvestmentThousands.toFixed(0)}K`} min={0} max={100} step={5} value={inputs.monthlyExtraInvestmentThousands} onChange={(value) => onInputChange("monthlyExtraInvestmentThousands", value)} />
      </section>

      <section className="home-plan-control-block">
        <span className="home-plan-control-kicker">Timing</span>
        <RangeControl label="Buy after" valueLabel={inputs.purchaseYear === 0 ? "Now" : `${inputs.purchaseYear} years`} min={0} max={7} step={1} value={inputs.purchaseYear} onChange={(value) => onInputChange("purchaseYear", value)} />
        <RangeControl label="Current rent" valueLabel={`₹${inputs.currentRentThousands.toFixed(0)}K / month`} min={15} max={150} step={5} value={inputs.currentRentThousands} onChange={(value) => onInputChange("currentRentThousands", value)} />
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
      label: "Property price",
      value: formatCurrency(details.price, true),
      note: `${details.price_per_sqft.toLocaleString("en-IN")} / sqft · listing input`,
      confidence: property.confidence_score?.label ?? "Model input",
    },
    {
      label: "Local benchmark",
      value: area?.median_price_per_sqft ? `₹${area.median_price_per_sqft.toLocaleString("en-IN")} / sqft` : "Not available",
      note: area?.trend_summary ?? "Area evidence is still being collected.",
      confidence: area ? "Area dataset" : "Missing",
    },
    {
      label: "Regulatory record",
      value: property.rera?.registered ? "RERA registered" : "Verification pending",
      note: property.rera?.registration_number ?? property.rera?.status ?? "No verified registration number available.",
      confidence: property.rera?.registered ? "Verified" : "Needs review",
    },
    {
      label: "Latest sourced fact",
      value: freshestFact?.value ?? "No sourced fact yet",
      note: freshestFact ? `${freshestFact.source_type} · ${freshestFact.confidence_pct}% confidence` : "The model uses editable assumptions until evidence arrives.",
      confidence: freshestFact?.learned_at ? `Updated ${new Date(freshestFact.learned_at).toLocaleDateString("en-IN")}` : "Freshness unavailable",
    },
  ];

  return (
    <section className="home-plan-control-block home-plan-sources-block">
      <span className="home-plan-control-kicker">Evidence drawer</span>
      <h3>What this plan knows</h3>
      <p>Property facts stay separate from editable financial assumptions so the result remains inspectable.</p>
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
  const sectionTitle = section === "financing" ? "Purchase and loan" : section === "market" ? "Market outlook" : "Evidence";

  return (
    <>
      {open && <button type="button" className="home-plan-controls-backdrop" aria-label="Close plan controls" onClick={onClose} />}
      <aside className={`home-plan-controls ${open ? "is-open" : ""}`} aria-hidden={!open}>
        <header className="home-plan-controls-header">
          <div><span>Assumptions</span><h2>{sectionTitle}</h2></div>
          <button type="button" onClick={onClose} aria-label="Close plan controls"><ControlIcon name="close" /></button>
        </header>
        <section className="home-plan-preset-selector" aria-label="Scenario presets">
          <div>
            <span>Starting point</span>
            <small>Choose a market view, then fine-tune any number below.</small>
          </div>
          <div>
            {(["Base scenario", "Cautious market", "Strong growth"] as const).map((item) => (
              <button type="button" key={item} className={preset === item ? "is-active" : ""} onClick={() => onPresetChange(item)}>
                {item === "Base scenario" ? "Base" : item === "Cautious market" ? "Cautious" : "Strong"}
              </button>
            ))}
          </div>
          {preset === "Custom" && <em>Custom assumptions</em>}
        </section>
        <nav className="home-plan-controls-tabs" aria-label="Plan control sections">
          {(["financing", "market", "sources"] as const).map((item) => (
            <button type="button" key={item} className={section === item ? "is-active" : ""} onClick={() => onSectionChange(item)}>
              <ControlIcon name={item} />
              <span>{item === "financing" ? "Purchase" : item === "market" ? "Market" : "Evidence"}</span>
            </button>
          ))}
        </nav>
        <div className="home-plan-controls-body">
          {section === "financing" && <FinancingControls inputs={inputs} projection={projection} extraEmisPerYear={extraEmisPerYear} onInputChange={onInputChange} onExtraEmisChange={onExtraEmisChange} />}
          {section === "market" && <MarketControls inputs={inputs} onInputChange={onInputChange} />}
          {section === "sources" && <SourcesPanel property={property} />}
        </div>
        <footer className="home-plan-controls-footer">
          <button type="button" onClick={onReset}>Reset to baseline</button>
          <button type="button" className="home-plan-primary-action" onClick={onClose}>Done</button>
        </footer>
      </aside>
    </>
  );
}
