export type MockProperty = {
  id: string;
  name: string;
  area: string;
  bhk: number;
  priceL: number;
  reason: string;
  image: string;
};

export type MockShelf = {
  id: string;
  title: string;
  quote: string;
  description: string;
  receipt: string;
  searchQuery: string;
  cards: MockProperty[];
};

const IMG = {
  a: "https://images.unsplash.com/photo-1560448204-e02f11c3d0e2?w=800&q=80",
  b: "https://images.unsplash.com/photo-1502672260266-1c1ef2e93688?w=800&q=80",
  c: "https://images.unsplash.com/photo-1522708323590-d24dbb6b0267?w=800&q=80",
  d: "https://images.unsplash.com/photo-1493809842364-78817add7ffb?w=800&q=80",
  e: "https://images.unsplash.com/photo-1484154218962-a197022b5858?w=800&q=80",
};

export const SHELVES: MockShelf[] = [
  {
    id: "verified_value",
    title: "Value with receipts",
    quote: "Good price, proof attached.",
    description: "Lower per-sqft options with visible source signals.",
    receipt: "Price + RERA + Google",
    searchQuery: "good value with proof",
    cards: [
      { id: "1", name: "Brigade Woods", area: "Whitefield", bhk: 1, priceL: 59, reason: "9485 /sqft · 2 sources", image: IMG.a },
      { id: "2", name: "Sobha Insignia", area: "Sarjapur", bhk: 3, priceL: 185, reason: "Strong price proof", image: IMG.b },
      { id: "3", name: "Godrej Splendour", area: "Whitefield", bhk: 3, priceL: 195, reason: "Below area median", image: IMG.c },
    ],
  },
  {
    id: "family_ready",
    title: "Family-ready societies",
    quote: "More life-fit, less guesswork.",
    description: "3BHK+ homes with society, risk, and review signals.",
    receipt: "Society + risk + reviews",
    searchQuery: "family friendly 3BHK",
    cards: [
      { id: "4", name: "Godrej Splendour", area: "Whitefield", bhk: 3, priceL: 195, reason: "Google 4.2 · society pulse", image: IMG.d },
      { id: "5", name: "Godrej Park Retreat", area: "Sarjapur", bhk: 3, priceL: 210, reason: "Low risk themes", image: IMG.e },
      { id: "6", name: "Prestige City", area: "Sarjapur", bhk: 4, priceL: 280, reason: "School access signal", image: IMG.a },
    ],
  },
  {
    id: "premium_explainable",
    title: "Premium but explainable",
    quote: "If it's expensive, it should explain itself.",
    description: "Higher-ticket homes with stronger proof or brand signals.",
    receipt: "Builder + trust facts",
    searchQuery: "premium explainable homes",
    cards: [
      { id: "7", name: "Century Ethos", area: "Hebbal", bhk: 4, priceL: 320, reason: "Builder delivery proof", image: IMG.b },
      { id: "8", name: "Shriram Esquire", area: "Koramangala", bhk: 3, priceL: 245, reason: "RERA + reviews aligned", image: IMG.c },
      { id: "9", name: "Embassy Pristine", area: "Bellandur", bhk: 3, priceL: 220, reason: "Premium with receipts", image: IMG.d },
    ],
  },
  {
    id: "area_tracker",
    title: "Area Tracker picks",
    quote: "Area signals first.",
    description: "Homes from active micro-markets with enough local context.",
    receipt: "Area crawl + local context",
    searchQuery: "area tracker picks",
    cards: [
      { id: "10", name: "Brigade Woods", area: "Whitefield", bhk: 1, priceL: 59, reason: "Active Whitefield supply", image: IMG.e },
      { id: "11", name: "Sobha Insignia", area: "Sarjapur", bhk: 3, priceL: 185, reason: "Sarjapur corridor heat", image: IMG.a },
      { id: "12", name: "Karle Zenith", area: "Thanisandra", bhk: 3, priceL: 165, reason: "Emerging north pocket", image: IMG.b },
    ],
  },
];

export function formatPrice(lakh: number): string {
  if (lakh >= 100) return `${(lakh / 100).toFixed(1)} Cr`;
  return `${lakh} L`;
}
