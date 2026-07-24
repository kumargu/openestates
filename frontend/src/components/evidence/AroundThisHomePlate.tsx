import { lazy, Suspense, useMemo, useState } from "react";
import type { PropertyMapContext } from "../../lib/types.ts";
import {
  availableLayers,
  buildNumberedPlaces,
  buildPlateViewport,
  clusterClosePlaces,
  compactPlaceLabel,
  filterPlacesByScale,
  layerLabel,
  placesForStory,
  resolveHomeAnchor,
  type PlateScaleMode,
  type PlateStory,
  type PlaceCluster,
} from "../../lib/nearbyPlateProjection.ts";

const AroundThisHomeMap = lazy(async () => {
  const module = await import("./AroundThisHomeMap.tsx");
  return { default: module.AroundThisHomeMap };
});

export function hasAroundThisHomePlate(context?: PropertyMapContext | null): boolean {
  return Boolean(
    context && (
      context.places.length > 0
      || context.water
      || (context.metro_lines?.length ?? 0) > 0
      || (context.green_patches?.length ?? 0) > 0
      || (context.lakes?.length ?? 0) > 0
    ),
  );
}

type AroundThisHomePlateProps = {
  context: PropertyMapContext;
};

export function AroundThisHomePlate({ context }: AroundThisHomePlateProps) {
  const layers = availableLayers(context);
  const home = resolveHomeAnchor(context);
  const [scale, setScale] = useState<PlateScaleMode>("nearby");
  const [story, setStory] = useState<PlateStory>({ kind: "essentials" });
  const [waterOn, setWaterOn] = useState(Boolean(context.water));
  const [greenOn, setGreenOn] = useState(
    Boolean((context.green_patches?.length ?? 0) > 0 || (context.lakes?.length ?? 0) > 0),
  );
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [openedClusterId, setOpenedClusterId] = useState<string | null>(null);

  const storyPlaces = useMemo(() => {
    const forStory = placesForStory(context, story);
    return filterPlacesByScale(forStory, scale);
  }, [context, story, scale]);

  const numbered = useMemo(() => buildNumberedPlaces(storyPlaces), [storyPlaces]);

  const { singles, clusters } = useMemo(() => {
    if (openedClusterId) {
      return { singles: numbered, clusters: [] as PlaceCluster[] };
    }
    return clusterClosePlaces(numbered, scale);
  }, [numbered, openedClusterId, scale]);

  const viewport = useMemo(() => {
    if (!home) {
      return {
        center: { latitude: 12.97, longitude: 77.59 },
        radiusKm: 1,
        zoom: 13,
        paddingFactor: 0.2,
      };
    }
    return buildPlateViewport(home, numbered, scale);
  }, [home, numbered, scale]);

  const selected =
    numbered.find((place) => place.id === selectedId)
    ?? numbered[0]
    ?? null;

  function selectStory(next: PlateStory) {
    setStory(next);
    setSelectedId(null);
    setOpenedClusterId(null);
    if (next.kind === "layer" && next.layer === "metro") {
      setScale("area");
    }
  }

  function selectPlace(id: string) {
    setSelectedId(id);
  }

  function selectCluster(cluster: PlaceCluster) {
    setOpenedClusterId(cluster.id);
    const first = cluster.placeIds[0];
    if (first) setSelectedId(first);
  }

  const showWater = Boolean(context.water && waterOn);
  const showMetroLines = Boolean(
    (context.metro_lines?.length ?? 0) > 0
    && (
      story.kind === "essentials"
      || (story.kind === "layer" && story.layer === "metro")
    ),
  );
  const showGreen = Boolean(
    greenOn
    && ((context.green_patches?.length ?? 0) > 0 || (context.lakes?.length ?? 0) > 0),
  );
  const canRenderMap = Boolean(home);

  return (
    <section className="nearby-plate" aria-label="Around this home">
      <div className="nearby-plate__head">
        <div>
          <p className="nearby-plate__kicker">Around this home</p>
          <h2 className="nearby-plate__title">{context.home.name}</h2>
          <p className="nearby-plate__sub">
            {context.home.area ? `${context.home.area} · ` : ""}
            real map · numbered places · source receipts
          </p>
        </div>
        <div className="nearby-plate__scale" role="group" aria-label="Map scale">
          <button
            type="button"
            className={`nearby-plate__scale-btn${scale === "nearby" ? " is-active" : ""}`}
            aria-pressed={scale === "nearby"}
            onClick={() => {
              setScale("nearby");
              setOpenedClusterId(null);
            }}
          >
            Nearby
          </button>
          <button
            type="button"
            className={`nearby-plate__scale-btn${scale === "area" ? " is-active" : ""}`}
            aria-pressed={scale === "area"}
            onClick={() => {
              setScale("area");
              setOpenedClusterId(null);
            }}
          >
            Area
          </button>
        </div>
      </div>

      <div className="nearby-plate__layers" role="toolbar" aria-label="Nearby story">
        <button
          type="button"
          className={`nearby-plate__chip${story.kind === "essentials" ? " is-active" : ""}`}
          aria-pressed={story.kind === "essentials"}
          onClick={() => selectStory({ kind: "essentials" })}
        >
          Essentials
        </button>
        {layers.map((layer) => {
          const on = story.kind === "layer" && story.layer === layer;
          return (
            <button
              key={layer}
              type="button"
              className={`nearby-plate__chip${on ? " is-active" : ""}`}
              aria-pressed={on}
              onClick={() => selectStory({ kind: "layer", layer })}
            >
              {layerLabel(layer)}
            </button>
          );
        })}
        {((context.green_patches?.length ?? 0) > 0 || (context.lakes?.length ?? 0) > 0) && (
          <button
            type="button"
            className={`nearby-plate__chip nearby-plate__chip--green${greenOn ? " is-active" : ""}`}
            aria-pressed={greenOn}
            onClick={() => setGreenOn((value) => !value)}
          >
            Green
          </button>
        )}
        {context.water && (
          <button
            type="button"
            className={`nearby-plate__chip nearby-plate__chip--water${waterOn ? " is-active" : ""}`}
            aria-pressed={waterOn}
            onClick={() => setWaterOn((value) => !value)}
          >
            Water
          </button>
        )}
      </div>

      <div className="nearby-plate__body">
        <div className="nearby-plate__canvas">
          {canRenderMap && home ? (
            <Suspense
              fallback={(
                <div className="nearby-plate__empty-map">
                  <p>Loading neighborhood map…</p>
                </div>
              )}
            >
              <AroundThisHomeMap
                key={`${home.latitude.toFixed(5)}-${home.longitude.toFixed(5)}`}
                home={{
                  latitude: home.latitude,
                  longitude: home.longitude,
                  name: context.home.name,
                }}
                homeApproximated={home.approximated}
                places={singles}
                clusters={clusters}
                selectedId={selected?.id ?? null}
                viewport={viewport}
                metroLines={context.metro_lines ?? []}
                greenPatches={context.green_patches ?? []}
                lakes={context.lakes ?? []}
                showMetroLines={showMetroLines}
                showGreen={showGreen}
                water={context.water}
                waterTint={showWater}
                onSelectPlace={selectPlace}
                onSelectCluster={selectCluster}
              />
            </Suspense>
          ) : (
            <div className="nearby-plate__empty-map">
              <p>Map coordinates are still enriching for this home.</p>
              <p>Receipts below still use the nearby place facts we have.</p>
            </div>
          )}

          {numbered.length > 0 && (
            <ol className="nearby-plate__nearest" aria-label="Numbered nearby places">
              {numbered.map((place) => {
                const isSelected = selected?.id === place.id;
                return (
                  <li key={place.id}>
                    <button
                      type="button"
                      className={`nearby-plate__nearest-row${isSelected ? " is-selected" : ""}`}
                      onClick={() => selectPlace(place.id)}
                    >
                      <span className="nearby-plate__nearest-num">{place.number}</span>
                      <span className="nearby-plate__nearest-copy">
                        <span className="nearby-plate__nearest-name">
                          {compactPlaceLabel(place.name)}
                        </span>
                        <span className="nearby-plate__nearest-meta">
                          {layerLabel(place.layer)}
                          {typeof place.distance_km === "number"
                            ? ` · ${place.distance_km.toFixed(1)} km`
                            : ""}
                          {typeof place.rating === "number"
                            ? ` · ${place.rating.toFixed(1)}`
                            : ""}
                        </span>
                      </span>
                    </button>
                  </li>
                );
              })}
            </ol>
          )}
        </div>

        <aside className="nearby-plate__receipt">
          {selected ? (
            <>
              <p className="nearby-plate__receipt-kicker">
                Place {selected.number} · receipt
              </p>
              <h3>{selected.name}</h3>
              <p className="nearby-plate__receipt-meta">
                {typeof selected.distance_km === "number"
                  ? `${selected.distance_km.toFixed(1)} km`
                  : "Distance pending"}
                {selected.note ? ` · ${selected.note}` : ""}
              </p>
              <dl className="nearby-plate__receipt-facts">
                <div>
                  <dt>Layer</dt>
                  <dd>{layerLabel(selected.layer)}</dd>
                </div>
                <div>
                  <dt>Source</dt>
                  <dd>{selected.source_type || "Google"}</dd>
                </div>
              </dl>
              {selected.source_url ? (
                <a
                  className="nearby-plate__receipt-link"
                  href={selected.source_url}
                  target="_blank"
                  rel="noreferrer"
                >
                  Open source
                </a>
              ) : (
                <p className="nearby-plate__receipt-note">
                  Source link pending for this place.
                </p>
              )}
            </>
          ) : (
            <p className="nearby-plate__receipt-note">
              Pick Essentials or a layer to read nearby places around this home.
            </p>
          )}

          {showGreen && (
            <div className="nearby-plate__green-card">
              <strong>Green nearby</strong>
              <span>
                {(context.green_patches?.length ?? 0) > 0
                  ? `${context.green_patches?.length} park${(context.green_patches?.length ?? 0) === 1 ? "" : "s"}`
                  : "No parks"}
                {" · "}
                {(context.lakes?.length ?? 0) > 0
                  ? `${context.lakes?.length} lake${(context.lakes?.length ?? 0) === 1 ? "" : "s"}`
                  : "No lakes"}
                {" · OpenStreetMap within 4 km"}
              </span>
            </div>
          )}

          {showWater && context.water && (
            <div className="nearby-plate__water-card">
              <strong>{context.water.groundwater_class} groundwater</strong>
              <span>
                {context.water.summary}
                {" · "}
                {context.water.source_type}
                {context.water.illustrative_zone
                  ? " · class is source-backed; zone geometry not drawn"
                  : ""}
              </span>
              {context.water.source_url && (
                <a
                  className="nearby-plate__receipt-link"
                  href={context.water.source_url}
                  target="_blank"
                  rel="noreferrer"
                >
                  Water source
                </a>
              )}
            </div>
          )}
        </aside>
      </div>
    </section>
  );
}
