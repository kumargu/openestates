type LandingCloseBannerProps = {
  query: string;
  onQueryChange: (value: string) => void;
  onSubmit: (event: React.FormEvent) => void;
};

export function LandingCloseBanner({
  query,
  onQueryChange,
  onSubmit,
}: LandingCloseBannerProps) {
  return (
    <section className="landing-close" aria-label="Search again">
      <div className="landing-close__panel">
        <h2>Fewer homes. Better reasons.</h2>
        <form
          onSubmit={onSubmit}
          className="landing-close__composer"
          aria-label="Search again"
          role="search"
        >
          <input
            className="landing-close__input"
            type="text"
            placeholder="Describe the home and life you want…"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            aria-label="Describe the property you are looking for"
          />
          <button type="submit" className="landing-close__submit" aria-label="Search">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M5 12h14M13 6l6 6-6 6" />
            </svg>
          </button>
        </form>
      </div>
    </section>
  );
}
