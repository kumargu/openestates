import { useRef, useState } from "react";
import { Link } from "react-router-dom";
import type { RecommendationBranch, RecommendationLens } from "../../lib/types.ts";
import { ImageWithFallback } from "../ImageWithFallback.tsx";
import { initialPropertySceneUrls } from "../../lib/propertyScene.ts";
import {
  BuildingIcon,
  ChevronIcon,
  RupeeIcon,
  SealIcon,
  TrainIcon,
} from "../evidence/EvidenceIcons.tsx";

function formatPrice(price: number): string {
  if (price >= 1_00_00_000) return `₹${(price / 1_00_00_000).toFixed(2)} Cr`;
  if (price >= 1_00_000) return `₹${(price / 1_00_000).toFixed(1)} L`;
  return `₹${price.toLocaleString("en-IN")}`;
}

const LENS_META: Record<
  RecommendationLens,
  { spine: string; icon: typeof SealIcon; gainLabel: string }
> = {
  proof: { spine: "trust", icon: SealIcon, gainLabel: "More proof" },
  value: { spine: "value", icon: RupeeIcon, gainLabel: "Better value" },
  trust: { spine: "trust", icon: BuildingIcon, gainLabel: "Safer file" },
  commute: { spine: "commute", icon: TrainIcon, gainLabel: "Closer commute" },
};

function branchImage(branch: RecommendationBranch): string {
  const urls = initialPropertySceneUrls({
    heroImage: branch.property.hero_image,
    societyId: branch.property.kg_entity_refs?.society_entity_id,
  });
  return urls[0] ?? branch.property.hero_image ?? "";
}

function PathCard({ branch }: { branch: RecommendationBranch }) {
  const meta = LENS_META[branch.lens];
  const Icon = meta.icon;
  const image = branchImage(branch);

  return (
    <Link
      to={`/property/${branch.property.id}`}
      className={`alt-path alt-path--${meta.spine}`}
    >
      <div className="alt-path__media">
        <ImageWithFallback
          src={image || null}
          alt={branch.property.title}
          className="alt-path__image"
          loading="lazy"
        />
        <span className="alt-path__vignette" aria-hidden="true" />
        <span className="alt-path__badge">
          <Icon size={13} />
          {meta.gainLabel}
        </span>
        <div className="alt-path__glass">
          <strong className="alt-path__name">{branch.property.title}</strong>
          <span className="alt-path__sub">
            {branch.property.society_name} · {branch.property.area}
          </span>
          <div className="alt-path__foot">
            <span className="alt-path__price">{formatPrice(branch.property.price)}</span>
            <span className="alt-path__hint">{branch.contrast}</span>
          </div>
        </div>
      </div>
    </Link>
  );
}

export function AlternativePaths({ branches }: { branches: RecommendationBranch[] }) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);

  if (branches.length === 0) return null;

  const scrollToIndex = (index: number) => {
    const track = trackRef.current;
    if (!track) return;
    const clamped = Math.max(0, Math.min(index, branches.length - 1));
    const card = track.children[clamped] as HTMLElement | undefined;
    card?.scrollIntoView({ behavior: "smooth", inline: "start", block: "nearest" });
    setActive(clamped);
  };

  const onScroll = () => {
    const track = trackRef.current;
    if (!track) return;
    const index = Math.round(track.scrollLeft / (track.clientWidth * 0.82));
    setActive(Math.max(0, Math.min(index, branches.length - 1)));
  };

  const multi = branches.length > 1;

  return (
    <section className="alt-paths">
      <div className="alt-paths__header">
        <div className="property-section-heading">
          <span>If this isn&apos;t quite right</span>
          <h2>Also worth a look</h2>
        </div>
        {multi && (
          <div className="alt-paths__nav">
            <button
              type="button"
              className="alt-paths__arrow"
              aria-label="Previous alternative"
              onClick={() => scrollToIndex(active - 1)}
              disabled={active === 0}
            >
              <ChevronIcon size={18} />
            </button>
            <button
              type="button"
              className="alt-paths__arrow alt-paths__arrow--next"
              aria-label="Next alternative"
              onClick={() => scrollToIndex(active + 1)}
              disabled={active === branches.length - 1}
            >
              <ChevronIcon size={18} />
            </button>
          </div>
        )}
      </div>

      <div
        className={`alt-paths__track${multi ? " alt-paths__track--slider" : ""}`}
        ref={trackRef}
        onScroll={multi ? onScroll : undefined}
      >
        {branches.map((branch) => (
          <PathCard key={`${branch.lens}-${branch.property.id}`} branch={branch} />
        ))}
      </div>

      {multi && (
        <div className="alt-paths__dots" role="tablist" aria-label="Alternatives">
          {branches.map((branch, index) => (
            <button
              key={`${branch.lens}-${branch.property.id}-dot`}
              type="button"
              className={`alt-paths__dot${index === active ? " alt-paths__dot--active" : ""}`}
              aria-label={`Go to alternative ${index + 1}`}
              aria-selected={index === active}
              role="tab"
              onClick={() => scrollToIndex(index)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
