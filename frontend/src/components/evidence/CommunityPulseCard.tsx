import type { CommunityPulse } from "../../lib/types.ts";
import { canShowBuyerSource, displaySourceType } from "../../lib/evidence.ts";
import { TrendDownIcon, TrendIcon } from "./EvidenceIcons.tsx";

type Props = {
  pulse: CommunityPulse;
};

function polarityLabel(polarity: string): string {
  if (polarity === "positive") return "Residents like";
  if (polarity === "concern") return "Worth checking";
  return "Resident note";
}

export function CommunityPulseCard({ pulse }: Props) {
  const positiveQuotes = pulse.quotes.filter((quote) => quote.polarity === "positive");
  const concernQuotes = pulse.quotes.filter((quote) => quote.polarity === "concern");
  const neutralQuotes = pulse.quotes.filter((quote) => quote.polarity === "neutral");
  const buyerVisibleSourceUrls = pulse.source_urls.filter((url) =>
    pulse.quotes.some((quote) => quote.source_url === url && canShowBuyerSource(quote.source_type)));

  return (
    <div className="community-pulse">
      <p className="community-pulse__paragraph">{pulse.paragraph}</p>

      {(pulse.positives.length > 0 || pulse.concerns.length > 0) && (
        <div className="community-pulse__themes">
          <span className="community-pulse__section-label">From reviews</span>
          {pulse.positives.length > 0 && (
            <div className="community-pulse__theme-row">
              <span className="community-pulse__theme-label community-pulse__theme-label--positive">
                <TrendIcon size={14} />
                Residents like
              </span>
              <div className="community-pulse__chips">
                {pulse.positives.map((theme) => (
                  <span key={theme} className="community-pulse__chip community-pulse__chip--positive">
                    {theme}
                  </span>
                ))}
              </div>
            </div>
          )}
          {pulse.concerns.length > 0 && (
            <div className="community-pulse__theme-row">
              <span className="community-pulse__theme-label community-pulse__theme-label--concern">
                <TrendDownIcon size={14} />
                Worth checking
              </span>
              <div className="community-pulse__chips">
                {pulse.concerns.map((theme) => (
                  <span key={theme} className="community-pulse__chip community-pulse__chip--concern">
                    {theme}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {[...positiveQuotes, ...concernQuotes, ...neutralQuotes].length > 0 && (
        <div className="community-pulse__quotes">
          {[...positiveQuotes, ...concernQuotes, ...neutralQuotes].map((quote) => (
            <blockquote key={quote.text} className={`community-pulse__quote community-pulse__quote--${quote.polarity}`}>
              <div className="community-pulse__quote-head">
                {quote.polarity === "positive" ? <TrendIcon size={14} /> : null}
                {quote.polarity === "concern" ? <TrendDownIcon size={14} /> : null}
                <p>{quote.text}</p>
              </div>
              <footer>
                <span>{polarityLabel(quote.polarity)}</span>
                {displaySourceType(quote.source_type) && (
                  <span>{displaySourceType(quote.source_type)}</span>
                )}
                {quote.source_url && canShowBuyerSource(quote.source_type) && (
                  <a href={quote.source_url} target="_blank" rel="noreferrer">
                    Source
                  </a>
                )}
              </footer>
            </blockquote>
          ))}
        </div>
      )}

      {buyerVisibleSourceUrls.length > 0 && (
        <div className="community-pulse__sources">
          {buyerVisibleSourceUrls.map((url) => (
            <a key={url} href={url} target="_blank" rel="noreferrer">
              Source
            </a>
          ))}
        </div>
      )}
    </div>
  );
}
