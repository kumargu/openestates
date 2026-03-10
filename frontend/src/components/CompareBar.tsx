/**
 * Floating compare tray at the bottom of the results page.
 * Shows selected properties (max 3) with a "Compare" CTA.
 */
import type { PropertyCard as PropertyCardType } from "../lib/types.ts";
import { ImageWithFallback } from "./ImageWithFallback.tsx";

function formatPrice(price: number): string {
  if (price >= 10_000_000) return `\u20B9${(price / 10_000_000).toFixed(1)} Cr`;
  if (price >= 100_000) return `\u20B9${(price / 100_000).toFixed(1)} L`;
  return `\u20B9${price.toLocaleString("en-IN")}`;
}

type Props = {
  items: PropertyCardType[];
  onRemove: (id: string) => void;
  onCompare: () => void;
  onClear: () => void;
};

export function CompareBar({ items, onRemove, onCompare, onClear }: Props) {
  if (items.length === 0) return null;

  return (
    <div className="compare-bar">
      <div className="compare-bar-inner">
        <div className="compare-bar-items">
          {items.map((item) => (
            <div key={item.id} className="compare-bar-chip">
              <div className="compare-bar-chip-img">
                <ImageWithFallback
                  src={item.hero_image}
                  alt={item.title}
                  style={{ width: "100%", height: "100%", objectFit: "cover" }}
                />
              </div>
              <div className="compare-bar-chip-info">
                <span className="compare-bar-chip-title">{item.title}</span>
                <span className="compare-bar-chip-price">{formatPrice(item.price)}</span>
              </div>
              <button
                className="compare-bar-chip-remove"
                onClick={() => onRemove(item.id)}
                aria-label={`Remove ${item.title} from compare`}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round">
                  <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          ))}

          {/* Empty slots */}
          {Array.from({ length: 3 - items.length }).map((_, i) => (
            <div key={`empty-${i}`} className="compare-bar-chip compare-bar-chip--empty">
              <span className="compare-bar-chip-empty-text">+ Add property</span>
            </div>
          ))}
        </div>

        <div className="compare-bar-actions">
          <button
            className="compare-bar-cta"
            onClick={onCompare}
            disabled={items.length < 2}
          >
            Compare {items.length >= 2 ? `(${items.length})` : ""}
          </button>
          <button className="compare-bar-clear" onClick={onClear}>
            Clear
          </button>
        </div>
      </div>
    </div>
  );
}
