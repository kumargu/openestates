import { formatCurrency } from "./model.ts";

type VerdictBlockProps = {
  activeYear: number;
  buyWins: boolean;
  advantage: number;
  isPreview: boolean;
  breakEvenYear: number | null;
  homeEquity: number;
  monthlyGapSummary: string;
};

export function VerdictBlock({
  activeYear,
  buyWins,
  advantage,
  isPreview,
  breakEvenYear,
  homeEquity,
  monthlyGapSummary,
}: VerdictBlockProps) {
  const leader = buyWins ? "Buying" : "Rent + mutual funds";
  const boundaryNote = breakEvenYear
    ? `Buying catches up in year ${breakEvenYear}.`
    : "Rent + invest stays ahead through year 20.";

  return (
    <header className="home-plan-verdict">
      <p className="home-plan-verdict__year">
        {isPreview ? "Previewing" : "Pinned at"} year {activeYear}
        {isPreview && <span className="home-plan-verdict__hint"> · click chart to pin</span>}
      </p>
      <h1 className="home-plan-verdict__headline">
        <span className="home-plan-verdict__leader">{leader}</span>
        {" "}is ahead by{" "}
        <span className="home-plan-verdict__amount">{formatCurrency(advantage, true)}</span>
      </h1>
      <p className="home-plan-verdict__detail">
        {boundaryNote} {formatCurrency(homeEquity, true)} in home equity by then. {monthlyGapSummary}.
      </p>
    </header>
  );
}
