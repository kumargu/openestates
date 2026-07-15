import { formatCurrency } from "./model.ts";

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
      <dl className="home-plan-verdict__breakdown" aria-label={`Year ${activeYear} breakdown`}>
        {isMonthly ? (
          <>
            <div className="home-plan-verdict__tile home-plan-verdict__tile--buy">
              <dt>Buy · EMI</dt>
              <dd>{formatCurrency(monthlyEmi)}/mo</dd>
              <small>Fixed until loan ends</small>
            </div>
            <div className="home-plan-verdict__tile home-plan-verdict__tile--rent">
              <dt>Rent</dt>
              <dd>{formatCurrency(monthlyRent)}/mo</dd>
              <small>Rises with inflation</small>
            </div>
          </>
        ) : (
          <>
            <div className="home-plan-verdict__tile home-plan-verdict__tile--buy">
              <dt>Buy path</dt>
              <dd>{formatCurrency(buyNetWorth, true)}</dd>
              <small>EMI {formatCurrency(monthlyEmi)}/mo</small>
            </div>
            <div className="home-plan-verdict__tile home-plan-verdict__tile--rent">
              <dt>Rent + SIP</dt>
              <dd>{formatCurrency(rentNetWorth, true)}</dd>
              <small>Rent {formatCurrency(monthlyRent)}/mo</small>
            </div>
          </>
        )}
      </dl>
    </header>
  );
}
