import type { CommunityPulse } from "../../lib/types.ts";

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

  return (
    <div className="community-pulse">
      <p className="community-pulse__paragraph">{pulse.paragraph}</p>

      {(pulse.positives.length > 0 || pulse.concerns.length > 0) && (
        <div className="community-pulse__themes">
          <span className="community-pulse__section-label">From reviews</span>
          {pulse.positives.length > 0 && (
            <div className="community-pulse__theme-row">
              <span className="community-pulse__theme-label">Residents like</span>
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
              <span className="community-pulse__theme-label">Worth checking</span>
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
              <p>{quote.text}</p>
              <footer>
                <span>{polarityLabel(quote.polarity)}</span>
                <span>{quote.source_type}</span>
                {quote.source_url && (
                  <a href={quote.source_url} target="_blank" rel="noreferrer">
                    Open source
                  </a>
                )}
              </footer>
            </blockquote>
          ))}
        </div>
      )}

      {pulse.source_urls.length > 0 && (
        <div className="community-pulse__sources">
          {pulse.source_urls.map((url) => (
            <a key={url} href={url} target="_blank" rel="noreferrer">
              Source
            </a>
          ))}
        </div>
      )}
    </div>
  );
}
