import { useEffect, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { PropertyPage } from "../../src/pages/PropertyPage.tsx";
import fixture from "../../fixtures/prestige-waterford-api/manifest.json";
import { getCatalog, getDetail } from "./api.ts";
import type { InventoryDetail } from "./generated/InventoryDetail.ts";
import { InventoryReceipts } from "./InventoryEvidence.tsx";
import { askingPrice, clue, labels, physicalFacts } from "./presentation.ts";
import "./preview.css";
import "./integrated.css";

type Load = { key: string; detail: InventoryDetail };

/** Dev-only binding: archived society context + explicitly selected example unit.
 * Imported only by Vite's inventory mode, never the production route bundle.
 */
export function IntegratedProperty() {
  const { id } = useParams();
  const [params] = useSearchParams();
  const scenario = params.get("scenario") ?? "four-ads";
  const [loaded, setLoaded] = useState<Load>();
  const [failure, setFailure] = useState<{ key: string; message: string }>();
  const [attempt, setAttempt] = useState(0);
  const key = `${scenario}:${attempt}`;
  useEffect(() => {
    if (id !== fixture.property_id) return;
    const controller = new AbortController();
    void getCatalog(controller.signal).then(async catalog => {
      const detail = await getDetail(scenario, catalog.snapshot_id, controller.signal);
      if (!controller.signal.aborted) setLoaded({ key, detail });
    }).catch((cause: unknown) => {
      if (!controller.signal.aborted) setFailure({ key, message: cause instanceof Error ? cause.message : "Asking prices couldn’t be loaded." });
    });
    return () => controller.abort();
  }, [id, scenario, key]);

  if (id !== fixture.property_id) return <PropertyPage />;
  const ready = loaded?.key === key ? loaded : undefined;
  const error = failure?.key === key ? failure.message : undefined;
  const summary = ready?.detail.summary;
  const copy = labels.integrated;
  const action = summary ? (summary.uncertain_count ? copy.uncertain
    : !summary.active_count ? copy.inactive
    : summary.active_count < summary.advertisement_count ? copy.partial.replace("{active}", String(summary.active_count)).replace("{count}", String(summary.advertisement_count))
    : summary.price_change ? clue(summary)
    : summary.active_count > 1 ? copy.compare.replace("{count}", String(summary.active_count))
    : copy.single) : "";

  return <div className="inventory-integrated">
    {ready && summary ? <PropertyPage homeIdentity={{
      propertyId: fixture.property_id,
      facts: physicalFacts(summary.home).map((value, index) => ({ key: String(index), value })),
      detail: {
        summary: <div className="inventory-integrated__ask">
          <span>{summary.ask ? copy.exampleAsk : copy.exampleUnavailable}</span>
          {summary.ask && <strong>{askingPrice(summary.ask)}</strong>}
        </div>,
        label: action,
        title: copy.title,
        content: <InventoryReceipts key={`${scenario}:${ready.detail.snapshot_id}`} detail={ready.detail} />,
      },
    }} /> : <div className="inventory-integrated__state" role={error ? "alert" : "status"}>
      <p>{error ?? "Loading home…"}</p>
      {error && <button type="button" onClick={() => setAttempt(value => value + 1)}>Try again</button>}
    </div>}
  </div>;
}
