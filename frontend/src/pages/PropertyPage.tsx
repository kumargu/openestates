import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import {
  Link,
  useNavigate,
  useParams,
  useSearchParams,
} from "react-router-dom";
import type {
  PropertyDetailResponse,
  ProofFocus,
  ArrivalSearchSociety,
  SurfaceSceneResponse,
} from "../lib/types.ts";
import {
  getProperty,
  getPropertySurface,
  getPropertySurfacesBatch,
  parseProofFocusParam,
  propertyDetailPath,
} from "../lib/api.ts";
import { PageState } from "../components/PageState.tsx";
import { PageTitle } from "../components/PageTitle.tsx";
import { AroundThisHomePlate } from "../components/evidence/AroundThisHomePlate.tsx";
import { NotebookCommentAnchor } from "../components/notebook/NotebookCommentAnchor.tsx";
import { SaveHeartButton } from "../components/SaveHeartButton.tsx";
import { PUBLIC_BRAND_NAME } from "../lib/brand.ts";
import { PropertyArrivalFilm } from "../components/property/PropertyArrivalFilm.tsx";
import { PropertyReviewsDeck } from "../components/property/PropertyReviewsDeck.tsx";
import {
  PropertySceneCard,
} from "../components/property/PropertySceneCard.tsx";
import {
  PropertySearchStrip,
} from "../components/property/PropertySearchRail.tsx";
import { BrandMark } from "../components/brand/BrandMark.tsx";
import {
  projectPropertyStory,
} from "../lib/propertyStory.ts";
import { hasAroundThisHomePlate } from "../lib/nearbyPlateProjection.ts";
import { propertyMapContextFromSurfaceScene } from "../lib/surfaceSceneProjection.ts";
import {
  discoveryMapContextForProperty,
  propertyExploreHref,
  readDiscoveryMapContext,
  requestSearchSpanReturn,
  searchSpanReturnDelta,
} from "../lib/navigationContext.ts";
import { formatListingPrice } from "../lib/listing-price.ts";
import {
  initialPropertySurfaceId,
  propertyProofMatch,
  propertySceneProofFocus,
} from "../lib/proof-focus.ts";
import { backendUrl, publicSiteUrl } from "../lib/runtimeConfig.ts";
import { useSearchSpan } from "../components/workspace/SearchSpanContext.ts";

function hasKnownNumber(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

const ARRIVAL_STORY_SURFACE_ID = "arrival_story";

function focusedEvidenceSource(data: PropertyDetailResponse, focus: ProofFocus) {
  for (const section of data.evidence?.sections ?? []) {
    const item = section.items.find((candidate) =>
      candidate.key?.toLocaleLowerCase("en-IN")
        === focus.factKey.toLocaleLowerCase("en-IN")
    );
    if (item?.source_url) return item.source_url;
  }
  return undefined;
}

function PropertySearchMatch({
  data,
  focus,
}: {
  data: PropertyDetailResponse;
  focus?: ProofFocus;
}) {
  if (focus?.targetId !== "property-search-match") return null;
  const value = focus.matchedValue?.trim() || focus.matchedLabel?.trim();
  if (!value) return null;
  const sourceUrl = focusedEvidenceSource(data, focus);
  return (
    <section
      id="property-search-match"
      className="property-fact-deck property-search-match"
      aria-label="Matched your search"
      tabIndex={-1}
    >
      <span>Matched your search</span>
      <strong>{value}</strong>
      {sourceUrl && (
        <a href={sourceUrl} target="_blank" rel="noreferrer">Source ↗</a>
      )}
    </section>
  );
}

function buildPropertyJsonLd(p: PropertyDetailResponse["property"]) {
  const sizeDescription = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft} sqft`
    : "available configuration";
  const jsonLd: Record<string, unknown> = {
    "@context": "https://schema.org",
    "@type": "RealEstateListing",
    name: p.title,
    description: p.description_summary || [
      hasKnownNumber(p.bhk) ? `${p.bhk} BHK` : null,
      sizeDescription,
      `in ${p.area}, ${p.city}`,
    ].filter(Boolean).join(", "),
    url: publicSiteUrl(`/property/${encodeURIComponent(p.id)}`),
    offers: {
      "@type": "Offer",
      price: p.price,
      priceCurrency: "INR",
    },
    address: {
      "@type": "PostalAddress",
      addressLocality: p.area,
      addressRegion: p.city,
    },
  };
  if (hasKnownNumber(p.bhk)) {
    jsonLd.numberOfRooms = p.bhk;
  }
  if (hasKnownNumber(p.carpet_area_sqft)) {
    jsonLd.floorSize = {
      "@type": "QuantitativeValue",
      value: p.carpet_area_sqft,
      unitCode: "FTK",
    };
  }
  if (p.hero_image) {
    jsonLd.image = backendUrl(p.hero_image);
  }
  return jsonLd;
}

export function PropertyPage() {
  const { id } = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();

  if (!id)
    return (
      <PageState
        variant="not_found"
        context="property"
        message="Property was not found."
      />
    );

  const focusParam = searchParams.get("focus");
  const contextId = searchParams.get("context");
  const contextQueryFingerprint = searchParams.get("qf");
  return (
    <PropertyPageBody
      key={`${id}:${focusParam ?? ""}:${contextId ?? ""}:${contextQueryFingerprint ?? ""}`}
      id={id}
      focusParam={focusParam}
      contextId={contextId}
      contextQueryFingerprint={contextQueryFingerprint}
    />
  );
}

function PropertyPageBody({
  id,
  focusParam,
  contextId,
  contextQueryFingerprint,
}: {
  id: string;
  focusParam: string | null;
  contextId: string | null;
  contextQueryFingerprint: string | null;
}) {
  const navigate = useNavigate();
  const propertySearchContext = useSearchSpan();
  const [storyPlaying, setStoryPlaying] = useState(true);
  const [data, setData] = useState<PropertyDetailResponse | null>(null);
  const proofFocus = useMemo(() => {
    const focus = parseProofFocusParam(focusParam);
    const detailBundleVersion = data?.evidence?.serving_bundle_version;
    if (
      detailBundleVersion
      && propertySearchContext
      && detailBundleVersion !== propertySearchContext.runtimeVersion.servingBundleVersion
    ) return undefined;
    return focus;
  }, [data?.evidence?.serving_bundle_version, focusParam, propertySearchContext]);
  const [aroundThisHomeScene, setAroundThisHomeScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [arrivalScene, setArrivalScene] =
    useState<SurfaceSceneResponse | null>(null);
  const [searchContextSocieties, setSearchContextSocieties] =
    useState<ArrivalSearchSociety[]>([]);
  const [status, setStatus] = useState<
    "loading" | "error" | "not_found" | "ok"
  >("loading");
  const [retryKey, setRetryKey] = useState(0);
  const discoveryMapContext = useMemo(
    () => discoveryMapContextForProperty(
      readDiscoveryMapContext(contextId),
      id,
      contextQueryFingerprint,
    ),
    [contextId, contextQueryFingerprint, id],
  );
  const currentSearchResult = propertySearchContext?.results.find(
    (result) => result.propertyId === id,
  );

  useLayoutEffect(() => {
    window.scrollTo({ top: 0, left: 0, behavior: "auto" });
  }, [id]);

  useEffect(() => {
    const controller = new AbortController();

    getProperty(id, { signal: controller.signal })
      .then((d) => {
        if (controller.signal.aborted) return;
        setData(d);
        setStatus("ok");
      })
      .catch((err: Error) => {
        if (controller.signal.aborted) return;
        setStatus(err.message.includes("404") ? "not_found" : "error");
      });

    return () => {
      controller.abort();
    };
  }, [id, retryKey]);

  useEffect(() => {
    const currentSocietyId = data?.entity_refs.society_entity_id;
    const candidates = discoveryMapContext?.candidates
      .filter((candidate) => candidate.societyId !== currentSocietyId) ?? [];
    if (candidates.length === 0) {
      let cancelled = false;
      void Promise.resolve().then(() => {
        if (!cancelled) setSearchContextSocieties([]);
      });
      return () => { cancelled = true; };
    }
    const controller = new AbortController();
    void getPropertySurfacesBatch(
      candidates.map((candidate) => candidate.propertyId),
      [ARRIVAL_STORY_SURFACE_ID],
    ).then((response) => {
      if (controller.signal.aborted) return;
      const scenesByPropertyId = new Map(response.items.map((item) => [
        item.propertyId,
        item.scenes.find((scene) => scene.surfaceId === ARRIVAL_STORY_SURFACE_ID),
      ]));
      const resolved = candidates.flatMap((candidate) => {
        const scene = scenesByPropertyId.get(candidate.propertyId);
        const mapContext = propertyMapContextFromSurfaceScene(scene);
        const latitude = mapContext?.home.latitude;
        const longitude = mapContext?.home.longitude;
        if (
          !Number.isFinite(latitude)
          || !Number.isFinite(longitude)
          || !latitude
          || !longitude
          || latitude < -90
          || latitude > 90
          || longitude < -180
          || longitude > 180
        ) return [];
        return [{
          href: propertyDetailPath(
            candidate.propertyId,
            candidate.proofFocus,
            discoveryMapContext?.id,
            discoveryMapContext?.queryFingerprint,
          ),
          propertyId: candidate.propertyId,
          societyId: candidate.societyId,
          proofFocus: candidate.proofFocus,
          preview: candidate.preview,
          home: {
            latitude,
            longitude,
            name: mapContext.home.name,
            boundary: mapContext.home.boundary,
          },
        } satisfies ArrivalSearchSociety];
      });
      setSearchContextSocieties(resolved.slice(0, 3));
    }).catch(() => {
      if (!controller.signal.aborted) setSearchContextSocieties([]);
    });
    return () => controller.abort();
  }, [data?.entity_refs.society_entity_id, discoveryMapContext]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertySurface(
      propertyId,
      initialPropertySurfaceId(proofFocus),
      propertySceneProofFocus(proofFocus),
    )
      .then((scene) => {
        if (!cancelled) setAroundThisHomeScene(scene);
      })
      .catch(() => {
        if (!cancelled) setAroundThisHomeScene(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id, proofFocus]);

  useEffect(() => {
    const propertyId = data?.property?.id;
    if (!propertyId) return;
    let cancelled = false;

    getPropertySurface(propertyId, ARRIVAL_STORY_SURFACE_ID)
      .then((scene) => {
        if (!cancelled) setArrivalScene(scene);
      })
      .catch(() => {
        if (!cancelled) setArrivalScene(null);
      });

    return () => {
      cancelled = true;
    };
  }, [data?.property?.id]);

  useEffect(() => {
    if (
      status !== "ok"
      || !proofFocus?.targetId
      || propertySearchContext
    ) return undefined;
    let secondFrame = 0;
    const firstFrame = window.requestAnimationFrame(() => {
      secondFrame = window.requestAnimationFrame(() => {
        const target = document.getElementById(proofFocus.targetId ?? "");
        if (!target) return;
        target.scrollIntoView({ block: "start" });
        target.focus({ preventScroll: true });
      });
    });
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame) window.cancelAnimationFrame(secondFrame);
    };
  }, [
    status,
    data?.property?.id,
    proofFocus,
    propertySearchContext,
    aroundThisHomeScene,
    arrivalScene,
  ]);

  if (status === "loading")
    return (
      <div
        className="property-story-page property-story-loading"
        aria-label="Loading property"
        aria-busy="true"
      >
        <section className="property-scene">
          <div className="property-story-loading__identity">
            <div>
              <div className="skeleton-bar property-story-loading__location" />
              <div className="skeleton-bar property-story-loading__title" />
            </div>
            <div className="property-story-loading__summary">
              <div className="skeleton-bar property-story-loading__facts" />
              <div className="skeleton-bar property-story-loading__actions" />
            </div>
          </div>
          <div className="skeleton-bar property-story-loading__media" />
        </section>
      </div>
    );
  if (status === "not_found")
    return (
      <PageState
        variant="not_found"
        context="property"
        message={`Property "${id}" was not found.`}
      />
    );
  if (status === "error")
    return (
      <PageState
        variant="error"
        context="property"
        onRetry={() => {
          setData(null);
          setStatus("loading");
          setRetryKey((current) => current + 1);
        }}
      />
    );
  if (!data) return null;

  const { property: p, society } = data;

  const pageTitle = hasKnownNumber(p.bhk)
    ? `${p.title} — ${p.bhk} BHK in ${p.area} | ${PUBLIC_BRAND_NAME}`
    : `${p.title} in ${p.area} | ${PUBLIC_BRAND_NAME}`;
  const pricePerSqftLabel = hasKnownNumber(p.price_per_sqft)
    ? `${p.price_per_sqft.toLocaleString("en-IN")} /sqft`
    : null;
  const sizeLabel = hasKnownNumber(p.carpet_area_sqft)
    ? `${p.carpet_area_sqft.toLocaleString("en-IN")} sqft`
    : null;
  const pageDescription = [
    hasKnownNumber(p.bhk) ? `${p.bhk} BHK` : null,
    sizeLabel,
    `in ${society?.name ? society.name + ", " : ""}${p.area}`,
    hasKnownNumber(p.price) ? formatListingPrice(p) : null,
    pricePerSqftLabel,
    `${p.area}, ${p.city}`,
  ]
    .filter(Boolean)
    .join(". ");
  const canonicalUrl = publicSiteUrl(`/property/${encodeURIComponent(p.id)}`);
  const socialImageUrl = p.hero_image ? backendUrl(p.hero_image) : null;
  const aroundThisHomeContext = propertyMapContextFromSurfaceScene(
    aroundThisHomeScene,
    data.map_context,
  );
  const arrivalContext = propertyMapContextFromSurfaceScene(
    arrivalScene,
    data.map_context,
  );
  const showNearbyPlate = hasAroundThisHomePlate(aroundThisHomeContext);
  const displayTitle = p.title.trim();
  const story = projectPropertyStory(data, {
    mapAvailable: showNearbyPlate,
  });
  const proofSourceUrl = proofFocus
    ? focusedEvidenceSource(data, proofFocus)
    : undefined;
  const residentVoiceMatch = propertyProofMatch(
    proofFocus,
    "resident-voice",
    proofSourceUrl,
  );
  const exploreHref = propertyExploreHref(p.area);
  const returnHref = propertySearchContext?.returnUrl ?? exploreHref;

  return (
    <div className="property-decision-page property-story-page">
      <PageTitle title={pageTitle} />
      <meta name="description" content={pageDescription} />
      <meta property="og:title" content={pageTitle} />
      <meta property="og:description" content={pageDescription} />
      <meta property="og:type" content="website" />
      <meta property="og:site_name" content={PUBLIC_BRAND_NAME} />
      <meta property="og:url" content={canonicalUrl} />
      <link rel="canonical" href={canonicalUrl} />
      {socialImageUrl && <meta property="og:image" content={socialImageUrl} />}
      <script type="application/ld+json">
        {JSON.stringify(buildPropertyJsonLd(p))}
      </script>
      <header className="property-journey-header">
        <Link to="/" className="property-journey-header__brand">
          <BrandMark size={30} />
          <span aria-hidden="true">{PUBLIC_BRAND_NAME}</span>
        </Link>
        <Link
          to={returnHref}
          className="property-journey-header__return"
          onClick={propertySearchContext
            ? (event) => {
              event.preventDefault();
              requestSearchSpanReturn(propertySearchContext);
              const delta = searchSpanReturnDelta(propertySearchContext);
              if (delta !== null) navigate(delta);
              else navigate(propertySearchContext.returnUrl, { replace: true });
            }
            : undefined}
        >
          <span aria-hidden="true">←</span>{propertySearchContext ? "Back to results" : "Explore homes"}
        </Link>
      </header>

      <div className="property-journey-layout">
        <div className="property-journey__canvas">
          {propertySearchContext && currentSearchResult ? (
            <PropertySearchStrip context={propertySearchContext} />
          ) : null}

          <PropertySceneCard
            sectionId="property-cover"
            story={story}
            identityPlacement="overlay"
            actions={(
              <>
                <SaveHeartButton
                  propertyId={p.id}
                  className="property-action-link property-action-save"
                  label="Save"
                />
                <NotebookCommentAnchor
                  propertyId={p.id}
                  labels={[]}
                  detail={displayTitle}
                  source="Property detail"
                  label="Note"
                />
              </>
            )}
            playback={{
              playing: storyPlaying,
              onPlayingChange: setStoryPlaying,
            }}
          />

          <main className="property-clean-flow">
            <PropertySearchMatch data={data} focus={proofFocus} />

            {story.map.available && aroundThisHomeContext && (
              <section
                id="around-this-home"
                className="property-map-section"
                aria-label="Around this home"
                tabIndex={-1}
              >
                <AroundThisHomePlate
                  propertyId={id}
                  context={aroundThisHomeContext}
                />
              </section>
            )}

            <PropertyArrivalFilm
              propertyId={p.id}
              title={story.identity.title}
              frames={story.arrival.frames}
              mapContext={arrivalContext}
              searchContextSocieties={searchContextSocieties}
              playback={{
                playing: storyPlaying,
                onPlayingChange: setStoryPlaying,
              }}
            />

            <PropertyReviewsDeck
              model={story.reviews}
              reviews={data.external_reviews}
              signals={data.detail_signals}
              focusedMatch={residentVoiceMatch}
            />

          </main>
        </div>

      </div>

    </div>
  );
}
