export type ListingPriceFields = {
  price: number;
  price_min?: number | null;
  price_max?: number | null;
};

function positiveNumber(value: number | null | undefined): number | undefined {
  return typeof value === "number" && Number.isFinite(value) && value > 0
    ? value
    : undefined;
}

export function listingPriceBounds(listing: ListingPriceFields): {
  low: number;
  high: number;
} {
  const low = positiveNumber(listing.price_min) ?? listing.price;
  const high = positiveNumber(listing.price_max) ?? listing.price;
  if (low <= 0 && high <= 0) return { low: 0, high: 0 };
  return low <= high ? { low, high } : { low: high, high: low };
}

export function listingSatisfiesBudget(
  listing: ListingPriceFields,
  budgetMin?: number | null,
  budgetMax?: number | null,
): boolean {
  const { low, high } = listingPriceBounds(listing);
  if (low <= 0 && high <= 0) return false;
  if (typeof budgetMin === "number" && budgetMin > 0 && high < budgetMin) {
    return false;
  }
  if (typeof budgetMax === "number" && budgetMax > 0 && low > budgetMax) {
    return false;
  }
  return true;
}

type PriceAmount = {
  text: string;
  amount: string;
  unit: "Cr" | "L" | "";
};

function formatPriceAmount(price: number): PriceAmount {
  if (price >= 10_000_000) {
    const amount = (price / 10_000_000).toFixed(1);
    return { text: `₹${amount} Cr`, amount, unit: "Cr" };
  }
  if (price >= 100_000) {
    const amount = (price / 100_000).toFixed(1);
    return { text: `₹${amount} L`, amount, unit: "L" };
  }
  const amount = price.toLocaleString("en-IN");
  return { text: `₹${amount}`, amount, unit: "" };
}

export function formatListingPrice(listing: ListingPriceFields): string {
  const { low, high } = listingPriceBounds(listing);
  if (low <= 0 && high <= 0) return "Price unavailable";
  const lowLabel = formatPriceAmount(low);
  const highLabel = formatPriceAmount(high);
  if (lowLabel.text === highLabel.text) return lowLabel.text;
  if (lowLabel.unit && lowLabel.unit === highLabel.unit) {
    return `₹${lowLabel.amount}–${highLabel.amount} ${lowLabel.unit}`;
  }
  return `${lowLabel.text}–${highLabel.text}`;
}
