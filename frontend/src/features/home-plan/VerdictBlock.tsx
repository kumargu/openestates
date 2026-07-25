import {
  formatCurrency,
  type BuilderPayment,
  type ConstructionProfile,
} from "./model.ts";

type VerdictBlockProps = {
  activeYear: number;
  buyWins: boolean;
  advantage: number;
  paymentSchedule: BuilderPayment[];
  possessionDate: string | null;
  constructionDateSource: ConstructionProfile["dateSource"];
  isUnderConstruction: boolean;
};

export function VerdictBlock({
  activeYear,
  buyWins,
  advantage,
  paymentSchedule,
  possessionDate,
  constructionDateSource,
  isUnderConstruction,
}: VerdictBlockProps) {
  const timeLabel = activeYear === 0
    ? "Today"
    : `After ${activeYear} ${activeYear === 1 ? "year" : "years"}`;
  const choice = buyWins ? "buy" : "rent and invest";
  const possessionLabel = possessionDate
    ? new Intl.DateTimeFormat("en-IN", { month: "short", year: "numeric", timeZone: "UTC" })
      .format(new Date(`${possessionDate}T00:00:00Z`))
    : null;
  const dueNow = paymentSchedule[0]?.amount ?? 0;
  const laterPayments = Math.max(0, paymentSchedule.length - 1);

  return (
    <header className="home-plan-verdict">
      <div className="home-plan-verdict__topline">
        <h1 className="home-plan-verdict__headline">
          {timeLabel}, you have{" "}
          <span className="home-plan-verdict__amount">{formatCurrency(advantage, true)} more</span>
          {" "}if you {choice}.
        </h1>
      </div>
      {isUnderConstruction && (
        <p className="home-plan-verdict__construction">
          Under construction · {formatCurrency(dueNow, true)} due now
          {laterPayments > 0 ? ` · ${laterPayments} more payments` : ""}
          {possessionLabel ? ` · possession ${possessionLabel}` : ""}
          . Payments are split about every 6 months
          {constructionDateSource === "estimated" ? " on an estimated schedule." : "."}
        </p>
      )}
    </header>
  );
}
