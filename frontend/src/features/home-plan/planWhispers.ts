export type PlanWhisperTheme = "balanced" | "buy" | "rent" | "prepay";

const NEUTRAL_WHISPERS = [
  "It was never just numbers. It's numbers plus how you sleep at night.",
  "Every good decision here is really a bet on your next ten years.",
  "There's no wrong answer. There's only the one that fits your life.",
  "The right answer changes the day your life does.",
  "A house is a decision. A home is everything after it.",
] as const;

const RENT_WHISPERS = [
  "Renting + investing quietly beats a lot of mortgages. Nobody posts about it.",
  "Rent buys you the option to leave. That's worth something too.",
  "Renting isn't throwing money away. You're paying to not fix the geyser at 2am.",
  "Liquidity is a feature you only appreciate when you need cash fast.",
  "Nithin Kamath built India's largest brokerage and rented his home for years.",
  "Mark Zuckerberg rented for years after Facebook's IPO. Optionality has fans in high places.",
] as const;

const BUY_WHISPERS = [
  "Owning feels like arriving. Renting feels like passing through.",
  "Equity builds slowly. Liquidity builds quietly. You need a little of both.",
  "The dream is the house. The fine print is the maintenance.",
  "Warren Buffett still lives in the Omaha house he bought in 1958.",
  "Some things a spreadsheet can't price. A place that's actually yours is one of them.",
  "Ghar toh ghar hota hai. Until the EMI reminds you it's also a loan.",
  "My parents measured settled in square feet. Fair enough.",
] as const;

const PREPAY_WHISPERS = [
  "EMI is forced saving. Rent is optionality. Pick your discipline.",
  "An EMI you can't feel is safe. One you can feel runs your calendar.",
  "Prepay when the surplus is real, not aspirational.",
  "The best loan is the one that ends before you stop earning.",
  "Freedom from EMI is a raise you give yourself.",
  "Extra EMIs today are tomorrow's peace of mind, if you can afford the tradeoff.",
  "Property is patient money. Make sure you're patient too.",
] as const;

const INVESTING_WHISPERS = [
  "SIPs are boring. Boring is underrated.",
  "The market doesn't care about your possession date.",
  "Time in the market beats timing the market, and timing the property market.",
  "The best return is often the mistake you didn't make.",
  "Compounding rewards patience the way real estate rewards location.",
] as const;

export function planWhispersFor(theme: PlanWhisperTheme): readonly string[] {
  if (theme === "prepay") return [...PREPAY_WHISPERS, ...NEUTRAL_WHISPERS];
  if (theme === "rent") return [...RENT_WHISPERS, ...INVESTING_WHISPERS, ...NEUTRAL_WHISPERS];
  if (theme === "buy") return [...BUY_WHISPERS, ...NEUTRAL_WHISPERS];
  return [...NEUTRAL_WHISPERS, ...BUY_WHISPERS, ...RENT_WHISPERS, ...INVESTING_WHISPERS];
}
