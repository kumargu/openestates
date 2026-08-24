export type PlanWhisperTheme = "balanced" | "buy" | "rent" | "prepay";

const NEUTRAL_WHISPERS = [
  "The spreadsheet calls it close. Your life gets the casting vote.",
  "Same starting capital. Different ways to lose sleep.",
  "The right answer changes when your life does.",
] as const;

const RENT_WHISPERS = [
  "The landlord owns the walls. You own the exit.",
  "The geyser remains someone else's 2 a.m. problem.",
  "Liquidity is boring right up until you need it.",
  "SIPs are boring. So are seatbelts. Both can be useful.",
] as const;

const BUY_WHISPERS = [
  "The keys are yours. The maintenance WhatsApp group is complimentary.",
  "The dream is the house. The fine print is the maintenance.",
  "The spreadsheet ends at net worth. Staying put does not.",
  "Ghar toh ghar hota hai. The EMI remains unconvinced.",
] as const;

const PREPAY_WHISPERS = [
  "Extra EMIs now. Fewer calendar reminders later.",
  "Prepay when the surplus is real, not aspirational.",
  "An EMI you can barely feel is the polite kind.",
  "Property is patient money. Make sure you are patient too.",
] as const;

const LOAN_FREE_WHISPER = "Your EMI has officially left the group chat.";

type PlanWhisperContext = {
  theme: PlanWhisperTheme;
  activeYear: number;
  loanFreeYear: number | null;
};

export function planWhispersFor(theme: PlanWhisperTheme): readonly string[] {
  if (theme === "prepay") return [...PREPAY_WHISPERS, ...NEUTRAL_WHISPERS];
  if (theme === "rent") return [...RENT_WHISPERS, ...NEUTRAL_WHISPERS];
  if (theme === "buy") return [...BUY_WHISPERS, ...NEUTRAL_WHISPERS];
  return [...NEUTRAL_WHISPERS, ...BUY_WHISPERS, ...RENT_WHISPERS];
}

export function planWhispersForContext({
  theme,
  activeYear,
  loanFreeYear,
}: PlanWhisperContext): readonly string[] {
  const whispers = planWhispersFor(theme);
  const start = Math.max(0, Math.round(activeYear)) % whispers.length;
  const ordered = [...whispers.slice(start), ...whispers.slice(0, start)];
  return loanFreeYear !== null && activeYear >= loanFreeYear
    ? [LOAN_FREE_WHISPER, ...ordered]
    : ordered;
}

export function planWhisperFor(context: PlanWhisperContext): string {
  return planWhispersForContext(context)[0] ?? NEUTRAL_WHISPERS[0];
}
