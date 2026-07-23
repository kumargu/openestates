import { formatCurrency } from "./model.ts";
import type { PlanScenarioId } from "./PlanGraph.tsx";

type VerdictBlockProps = {
  view: "netWorth" | "monthly";
  activeYear: number;
  buyWins: boolean;
  advantage: number;
  isPreview: boolean;
  breakEvenYear: number | null;
  homeEquity: number;
  monthlyGap: number;
  monthlyGapSummary: string;
  changeNote: string | null;
  monthlyEmi: number;
  monthlyRent: number;
  buyNetWorth: number;
  rentNetWorth: number;
  selectedScenario: PlanScenarioId;
  onSelectScenario: (scenario: PlanScenarioId) => void;
};

export function VerdictBlock({
  view,
  activeYear,
  buyWins,
  advantage,
  isPreview,
  breakEvenYear,
  homeEquity,
  monthlyGap,
  monthlyGapSummary,
  changeNote,
  monthlyEmi,
  monthlyRent,
  buyNetWorth,
  rentNetWorth,
  selectedScenario,
  onSelectScenario,
}: VerdictBlockProps) {
  const isMonthly = view === "monthly";

  const leader = isMonthly
    ? (monthlyGap <= 0 ? "Buying" : "Renting")
    : (buyWins ? "Buying" : "Renting and investing");

  const headlineTail = isMonthly
    ? "costs less each month"
    : "leads by";

  const amount = isMonthly
    ? `${formatCurrency(Math.abs(monthlyGap))}/mo`
    : formatCurrency(advantage, true);

  const detailLine = isMonthly
    ? "Rent rises with inflation each year, while your EMI stays fixed until the loan ends."
    : breakEvenYear
      ? `Buying catches up in year ${breakEvenYear}. You would hold ${formatCurrency(homeEquity, true)} in home equity by year ${activeYear}.`
      : `Renting and investing stays ahead for the full 20 years. Home equity would be ${formatCurrency(homeEquity, true)} by year ${activeYear}.`;

  return (
    <header className="home-plan-verdict">
      <p className="home-plan-verdict__year">
        {isPreview ? `Year ${activeYear} (preview)` : `Year ${activeYear}`}
        {isPreview && <span className="home-plan-verdict__hint"> — tap the chart to keep this year</span>}
      </p>
      <h1 className="home-plan-verdict__headline">
        <span className="home-plan-verdict__leader">{leader}</span>
        {" "}{headlineTail}{" "}
        <span className="home-plan-verdict__amount">{amount}</span>
      </h1>
      <p className="home-plan-verdict__detail">{detailLine}</p>
      {!isMonthly && <p className="home-plan-verdict__cashflow">{monthlyGapSummary}.</p>}
      {changeNote && <p className="home-plan-verdict__change">{changeNote}</p>}
      <div className="home-plan-verdict__breakdown" role="group" aria-label={`Year ${activeYear} paths`}>
        {isMonthly ? (
          <>
            <button
              type="button"
              className={`home-plan-verdict__tile home-plan-verdict__tile--buy ${selectedScenario === "buy" ? "is-selected" : ""}`}
              onClick={() => onSelectScenario("buy")}
              aria-pressed={selectedScenario === "buy"}
            >
              <span className="home-plan-verdict__tile-label"><i />Buy · EMI</span>
              <strong>{formatCurrency(monthlyEmi)}/mo</strong>
              <small>Fixed until loan ends</small>
            </button>
            <button
              type="button"
              className={`home-plan-verdict__tile home-plan-verdict__tile--rent ${selectedScenario === "rent" ? "is-selected" : ""}`}
              onClick={() => onSelectScenario("rent")}
              aria-pressed={selectedScenario === "rent"}
            >
              <span className="home-plan-verdict__tile-label"><i />Rent</span>
              <strong>{formatCurrency(monthlyRent)}/mo</strong>
              <small>Rises with inflation</small>
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              className={`home-plan-verdict__tile home-plan-verdict__tile--buy ${selectedScenario === "buy" ? "is-selected" : ""}`}
              onClick={() => onSelectScenario("buy")}
              aria-pressed={selectedScenario === "buy"}
            >
              <span className="home-plan-verdict__tile-label"><i />Buy path</span>
              <strong>{formatCurrency(buyNetWorth, true)}</strong>
              <small>EMI {formatCurrency(monthlyEmi)}/mo</small>
            </button>
            <button
              type="button"
              className={`home-plan-verdict__tile home-plan-verdict__tile--rent ${selectedScenario === "rent" ? "is-selected" : ""}`}
              onClick={() => onSelectScenario("rent")}
              aria-pressed={selectedScenario === "rent"}
            >
              <span className="home-plan-verdict__tile-label"><i />Rent + SIP</span>
              <strong>{formatCurrency(rentNetWorth, true)}</strong>
              <small>Rent {formatCurrency(monthlyRent)}/mo</small>
            </button>
          </>
        )}
      </div>
    </header>
  );
}
