export function formatGoogleRating(
  value: number | null | undefined,
): string | null {
  if (
    typeof value !== "number"
    || !Number.isFinite(value)
    || value <= 0
  ) {
    return null;
  }
  return value.toFixed(1);
}
