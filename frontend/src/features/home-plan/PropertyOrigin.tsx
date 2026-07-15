import { Link } from "react-router-dom";
import { formatCurrency, type PlanInputs } from "./model.ts";

type PropertyOriginProps = {
  propertyId: string;
  title: string;
  area: string;
  bhk: number;
  price: number;
  inputs: PlanInputs;
  presetLabel: string;
};

export function PropertyOrigin({
  propertyId,
  title,
  area,
  bhk,
  price,
  inputs,
  presetLabel,
}: PropertyOriginProps) {
  return (
    <Link to={`/property/${propertyId}`} className="home-plan-origin">
      <span className="home-plan-origin__marker" aria-hidden="true" />
      <div className="home-plan-origin__body">
        <span className="home-plan-origin__kicker">
          {area} · {bhk} BHK · {presetLabel}
        </span>
        <strong className="home-plan-origin__title">{title}</strong>
        <span className="home-plan-origin__price">
          {formatCurrency(price, true)}
          <small>· ₹{inputs.downPaymentLakh.toFixed(0)}L down</small>
        </span>
      </div>
      <span className="home-plan-origin__chevron" aria-hidden="true">›</span>
    </Link>
  );
}
