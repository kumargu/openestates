import { Link } from "react-router-dom";
import { formatCurrency } from "./model.ts";

type PropertyOriginProps = {
  propertyId: string;
  title: string;
  area: string;
  price: number;
};

export function PropertyOrigin({ propertyId, title, area, price }: PropertyOriginProps) {
  return (
    <Link to={`/property/${propertyId}`} className="home-plan-origin">
      <span className="home-plan-origin__marker" aria-hidden="true" />
      <span className="home-plan-origin__label">
        <span className="home-plan-origin__place">{area}</span>
        <span className="home-plan-origin__sep" aria-hidden="true">·</span>
        <span className="home-plan-origin__title">{title}</span>
      </span>
      <span className="home-plan-origin__price">{formatCurrency(price, true)}</span>
      <span className="home-plan-origin__chevron" aria-hidden="true">›</span>
    </Link>
  );
}
