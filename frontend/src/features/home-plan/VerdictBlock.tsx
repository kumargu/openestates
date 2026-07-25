import {
  formatCurrency,
  type BuilderPayment,
  type ConstructionProfile,
} from "./model.ts";
import type { PlanScenarioId } from "./PlanGraph.tsx";

type VerdictBlockProps = {
  activeYear: number;
  buyWins: boolean;
  advantage: number;
  isPreview: boolean;
  breakEvenYear: number | null;
  homeEquity: number;
  monthlyGapSummary: string;
  changeNote: string | null;
  monthlyEmi: number;
  monthlyRent: number;
  buyNetWorth: number;
  rentNetWorth: number;
  selectedScenario: PlanScenarioId;
  paymentSchedule: BuilderPayment[];
  possessionDate: string | null;
  constructionDateSource: ConstructionProfile["dateSource"];
  isUnderConstruction: boolean;
  isBeforePossession: boolean;
  onSelectScenario: (scenario: PlanScenarioId) => void;
};

export function VerdictBlock({
  activeYear,
  buyWins,
  advantage,
  isPreview,
  breakEvenYear,
  homeEquity,
  monthlyGapSummary,
  changeNote,
  monthlyEmi,
  monthlyRent,
  buyNetWorth,
  rentNetWorth,
  selectedScenario,
  paymentSchedule,
  possessionDate,
  constructionDateSource,
  isUnderConstruction,
  isBeforePossession,
  onSelectScenario,
}: VerdictBlockProps) {
  const betterPath = buyWins ? "buying this home" : "renting and investing";
  const detailLine = buyWins
    ? `Estimated net worth: buy ${formatCurrency(buyNetWorth, true)} · rent and invest ${formatCurrency(rentNetWorth, true)}.`
    : breakEvenYear
      ? `Buying catches up around year ${breakEvenYear}. Home equity at year ${activeYear}: ${formatCurrency(homeEquity, true)}.`
      : `Buying does not catch up within this plan. Home equity at year ${activeYear}: ${formatCurrency(homeEquity, true)}.`;
  const possessionLabel = possessionDate
    ? new Intl.DateTimeFormat("en-IN", { month: "short", year: "numeric", timeZone: "UTC" })
      .format(new Date(`${possessionDate}T00:00:00Z`))
    : null;
  const dueNow = paymentSchedule[0]?.amount ?? 0;
  const laterPayments = Math.max(0, paymentSchedule.length - 1);

  return (
    <header className="home-plan-verdict">
      <p className="home-plan-verdict__year">
        {isPreview ? `Year ${activeYear} (preview)` : `Year ${activeYear}`}
      </p>
      <h1 className="home-plan-verdict__headline">
        At year {activeYear}, {betterPath} leaves you{" "}
        <span className="home-plan-verdict__amount">{formatCurrency(advantage, true)} better off</span>.
      </h1>
      <p className="home-plan-verdict__detail">{detailLine}</p>
      {isUnderConstruction && (
        <p className="home-plan-verdict__construction">
          Under construction · {formatCurrency(dueNow, true)} due now
          {laterPayments > 0 ? ` · ${laterPayments} more payments` : ""}
          {possessionLabel ? ` · possession ${possessionLabel}` : ""}
          . Payments are split about every 6 months
          {constructionDateSource === "rera" ? " using RERA dates." : " using an estimated schedule."}
        </p>
      )}
      <p className="home-plan-verdict__cashflow">{monthlyGapSummary}.</p>
      {changeNote && <p className="home-plan-verdict__change">{changeNote}</p>}
      <div className="home-plan-verdict__breakdown" role="group" aria-label={`Year ${activeYear} paths`}>
        <button
          type="button"
          className={`home-plan-verdict__tile home-plan-verdict__tile--buy ${selectedScenario === "buy" ? "is-selected" : ""}`}
          onClick={() => onSelectScenario("buy")}
          aria-pressed={selectedScenario === "buy"}
        >
          <span className="home-plan-verdict__tile-label"><i />Buy</span>
          <strong>{formatCurrency(buyNetWorth, true)}</strong>
          <small>
            {isBeforePossession ? "EMI after possession " : "EMI "}
            {formatCurrency(monthlyEmi)}/mo
          </small>
        </button>
        <button
          type="button"
          className={`home-plan-verdict__tile home-plan-verdict__tile--rent ${selectedScenario === "rent" ? "is-selected" : ""}`}
          onClick={() => onSelectScenario("rent")}
          aria-pressed={selectedScenario === "rent"}
        >
          <span className="home-plan-verdict__tile-label"><i />Rent + invest</span>
          <strong>{formatCurrency(rentNetWorth, true)}</strong>
          <small>Rent {formatCurrency(monthlyRent)}/mo</small>
        </button>
      </div>
    </header>
  );
}
