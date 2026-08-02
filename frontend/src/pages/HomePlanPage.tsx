import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getProperties, getProperty } from "../lib/api.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import { PageState } from "../components/PageState.tsx";
import { NotebookSaveIcon } from "../components/notebook/NotebookSaveIcon.tsx";
import { WorkspaceHeader } from "../components/workspace/WorkspaceHeader.tsx";
import { useNotebook } from "../hooks/useNotebook.ts";
import { readShortlistIds, writeShortlistIds } from "../lib/compare.ts";
import { workspaceBuyVsRentHref, workspaceCompareHref } from "../lib/workspaceNav.ts";
import { PlanAssumptionRail } from "../features/home-plan/PlanAssumptionRail.tsx";
import { PlanGraph } from "../features/home-plan/PlanGraph.tsx";
import { PlanWhisper } from "../features/home-plan/PlanWhisper.tsx";
import { VerdictBlock } from "../features/home-plan/VerdictBlock.tsx";
import {
  buildBaselinePlanInputs,
  calculateProjection,
  formatCurrency,
  type ConstructionProfile,
  type PlanInputs,
} from "../features/home-plan/model.ts";
import "../features/home-plan/home-plan.css";
import { BUY_VS_RENT } from "../features/home-plan/labels.ts";
import {
  isExplicitlyReadyStatus,
  parsePlanDate,
} from "../features/home-plan/financeEngine.ts";
import { buildMonthlyPlanVerdict, defaultPlanFocusYear } from "../features/home-plan/monthlyPlanView.ts";
import { readPlanDraft, writePlanDraft } from "../features/home-plan/planDrafts.ts";
import { buildPlanSnapshotNote } from "../features/home-plan/planSnapshot.ts";

function constructionProfileFor(data: PropertyDetailResponse): ConstructionProfile {
  const asOfDate = new Date().toISOString().slice(0, 10);
  const stateText = [
    data.project_status,
    data.property.possession_status,
    data.rera?.status,
    data.home_state_display,
  ].filter(Boolean).join(" ").toLowerCase();
  const explicitlyReady = [
    data.project_status,
    data.property.possession_status,
    data.rera?.status,
    data.home_state_display,
  ].filter(Boolean).some((value) => (
    isExplicitlyReadyStatus(String(value))
  ));
  const completionDate = data.rera?.completion_date;
  const parsedCompletion = parsePlanDate(completionDate);
  const completionIsFuture = parsedCompletion ? parsedCompletion.getTime() > Date.now() : false;
  const underConstruction = !explicitlyReady && (
    completionIsFuture
    || ["under construction", "under_construction", "ongoing", "new launch"]
      .some((term) => stateText.includes(term))
  );

  return {
    state: underConstruction ? "under_construction" : "ready",
    asOfDate,
    startDate: data.rera?.start_date,
    completionDate,
    dateSource: completionDate ? "rera" : underConstruction ? "estimated" : "not_applicable",
  };
}

function LoadingPlan() {
  return (
    <div className="home-plan-loading" aria-label={BUY_VS_RENT.loading}>
      <div className="home-plan-loading-pane">
        <div />
        <div />
        <div />
      </div>
    </div>
  );
}

function displayName(home: PropertyCard): string {
  return home.society_name?.trim() || home.title;
}

function PlanPropertyContext({
  propertyId,
  property,
  homes,
  onSelect,
}: {
  propertyId?: string;
  property?: PropertyDetailResponse["property"];
  homes: PropertyCard[];
  onSelect: (propertyId: string) => void;
}) {
  return (
    <div className="workspace-plan-context">
      <label>
        <span className="sr-only">Home for Buy vs Rent</span>
        <select
          value={propertyId ?? ""}
          onChange={(event) => onSelect(event.target.value)}
          aria-label="Home for Buy vs Rent"
        >
          {!propertyId && <option value="">Choose a home</option>}
          {homes.map((home) => (
            <option key={home.id} value={home.id}>
              {displayName(home)}
            </option>
          ))}
        </select>
      </label>
      {property && (
        <span>{property.area} · {formatCurrency(property.price, true)}</span>
      )}
    </div>
  );
}

function PlanAssumptionsSheet({
  open,
  inputs,
  extraEmisPerYear,
  onInputChange,
  onExtraEmisChange,
  onReset,
  onClose,
}: {
  open: boolean;
  inputs: PlanInputs;
  extraEmisPerYear: number;
  onInputChange: <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => void;
  onExtraEmisChange: (count: number) => void;
  onReset: () => void;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const previousFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    closeRef.current?.focus();
    const handleDialogKeys = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const sheet = closeRef.current?.closest<HTMLElement>(".plan-assumptions-sheet");
      const focusable = sheet
        ? [...sheet.querySelectorAll<HTMLElement>("button, input, select, textarea, a[href], [tabindex]:not([tabindex='-1'])")]
          .filter((element) => !element.hasAttribute("disabled"))
        : [];
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", handleDialogKeys);
    return () => {
      document.removeEventListener("keydown", handleDialogKeys);
      previousFocus?.focus();
    };
  }, [onClose, open]);

  if (!open) return null;
  return (
    <div className="plan-assumptions-layer">
      <button
        type="button"
        className="plan-assumptions-backdrop"
        aria-label="Close assumptions"
        onClick={onClose}
      />
      <aside
        className="plan-assumptions-sheet"
        role="dialog"
        aria-modal="true"
        aria-labelledby="plan-assumptions-title"
      >
        <header>
          <div>
            <h2 id="plan-assumptions-title">Assumptions</h2>
            <p>Changes update the outcome immediately.</p>
          </div>
          <button ref={closeRef} type="button" onClick={onClose} aria-label="Close assumptions">
            ×
          </button>
        </header>
        <PlanAssumptionRail
          inputs={inputs}
          extraEmisPerYear={extraEmisPerYear}
          onInputChange={onInputChange}
          onExtraEmisChange={onExtraEmisChange}
          onReset={onReset}
        />
      </aside>
    </div>
  );
}

export function HomePlanPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { compareIds, isPinned, propertyIds, toggleFact } = useNotebook();
  const [catalog, setCatalog] = useState<PropertyCard[]>([]);
  const [propertyData, setPropertyData] = useState<PropertyDetailResponse | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "not_found" | "error">("loading");
  const [inputs, setInputs] = useState<PlanInputs | null>(null);
  const [previewYear, setPreviewYear] = useState<number | null>(null);
  const [pinnedYear, setPinnedYear] = useState<number | null>(null);
  const [extraEmisPerYear, setExtraEmisPerYear] = useState(0);
  const [assumptionsOpen, setAssumptionsOpen] = useState(false);
  const closeAssumptions = useCallback(() => setAssumptionsOpen(false), []);

  useEffect(() => {
    const controller = new AbortController();
    getProperties({ signal: controller.signal })
      .then(setCatalog)
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        setCatalog([]);
      });
    return () => controller.abort();
  }, []);

  useEffect(() => {
    if (!id) return undefined;
    let active = true;
    queueMicrotask(() => {
      if (!active) return;
      setStatus("loading");
      setAssumptionsOpen(false);
    });
    getProperty(id)
      .then((data) => {
        if (!active) return;
        const baseline = buildBaselinePlanInputs(data.property.price, constructionProfileFor(data));
        const draft = readPlanDraft(id);
        setPropertyData(data);
        setInputs(draft ? {
          ...draft.inputs,
          propertyPriceLakh: baseline.propertyPriceLakh,
          construction: baseline.construction,
        } : baseline);
        setPreviewYear(null);
        setPinnedYear(null);
        setExtraEmisPerYear(draft?.extraEmisPerYear ?? 0);
        const savedIds = readShortlistIds();
        if (!savedIds.includes(id)) writeShortlistIds([id, ...savedIds]);
        setStatus("ready");
      })
      .catch((error: unknown) => {
        if (!active) return;
        const message = error instanceof Error ? error.message : "";
        setStatus(message.includes("404") ? "not_found" : "error");
      });
    return () => { active = false; };
  }, [id]);

  useEffect(() => {
    if (!id || status !== "ready" || !inputs) return;
    writePlanDraft(id, inputs, extraEmisPerYear);
  }, [extraEmisPerYear, id, inputs, status]);

  const projection = useMemo(
    () => inputs ? calculateProjection(inputs, extraEmisPerYear) : null,
    [inputs, extraEmisPerYear],
  );

  const workspacePropertyIds = [...new Set([...(id ? [id] : []), ...propertyIds])];
  const homes = workspacePropertyIds
    .map((propertyId) => catalog.find((home) => home.id === propertyId))
    .filter((home): home is PropertyCard => Boolean(home));
  const compareHref = workspaceCompareHref(compareIds, id);
  const buyVsRentHref = workspaceBuyVsRentHref(id ?? propertyIds[0]);

  const selectProperty = (propertyId: string) => {
    if (propertyId) navigate(workspaceBuyVsRentHref(propertyId));
  };

  if (!id || status !== "ready" || !propertyData || !inputs || !projection) {
    const content = !id ? (
      <section className="home-plan-empty">
        <h1>Choose a home to plan.</h1>
        <p>Buy vs Rent uses the price and status of one home from your workspace.</p>
        {homes.length === 0 && <Link to="/">Discover homes</Link>}
      </section>
    ) : status === "loading" ? (
      <LoadingPlan />
    ) : status === "not_found" ? (
      <PageState variant="not_found" context="property" message={BUY_VS_RENT.unavailable} />
    ) : (
      <PageState variant="error" context="property" message={BUY_VS_RENT.loadError} />
    );

    return (
      <div className="home-plan-shell home-plan-shell--workspace">
        <Helmet>
          <title>{BUY_VS_RENT.pageTitle} | OpenEstates</title>
          <meta name="robots" content="noindex" />
        </Helmet>
        <WorkspaceHeader
          mode="buy-vs-rent"
          compareHref={compareHref}
          buyVsRentHref={buyVsRentHref}
          compareCount={compareIds.length}
          context={(
            <PlanPropertyContext homes={homes} onSelect={selectProperty} />
          )}
        />
        {content}
      </div>
    );
  }

  const property = propertyData.property;
  const baseline = buildBaselinePlanInputs(property.price, constructionProfileFor(propertyData));
  const defaultYear = defaultPlanFocusYear(projection, inputs.holdingPeriodYears);
  const activeYear = previewYear ?? pinnedYear ?? defaultYear;
  const verdict = buildMonthlyPlanVerdict(projection, activeYear);
  const whisperTheme = projection.extraEmisPerYear > 0
    ? "prepay"
    : verdict.buyWins
      ? "buy"
      : "rent";
  const whisperSignature = [
    whisperTheme,
    verdict.activeYear,
    projection.loanFreeYear ?? "open",
    Math.round(verdict.advantage),
  ].join(":");
  const planSnapshot = buildPlanSnapshotNote({
    propertyId: id,
    propertyTitle: property.title,
    inputs,
    projection,
    activeYear: verdict.activeYear,
  });
  const snapshotSaved = isPinned(planSnapshot.catalogKey);

  const updateInput = <K extends keyof PlanInputs>(key: K, value: PlanInputs[K]) => {
    setPreviewYear(null);
    setPinnedYear(null);
    setInputs((current) => current ? { ...current, [key]: value } : current);
  };

  const updateExtraEmisPerYear = (count: number) => {
    setPreviewYear(null);
    setPinnedYear(null);
    setExtraEmisPerYear(count);
  };

  const resetInputs = () => {
    setInputs(baseline);
    setPreviewYear(null);
    setPinnedYear(null);
    setExtraEmisPerYear(0);
  };

  const toggleSnapshot = () => {
    toggleFact({
      propertyId: id,
      catalogKey: planSnapshot.catalogKey,
      title: planSnapshot.title,
      detail: planSnapshot.detail,
      source: planSnapshot.source,
      labels: planSnapshot.labels,
      kind: "plan",
    });
  };

  return (
    <div className="home-plan-shell home-plan-shell--workspace">
      <Helmet>
        <title>{property.title} — {BUY_VS_RENT.pageTitle} | OpenEstates</title>
        <meta name="description" content={`Compare buying ${property.title} with renting and investing over time.`} />
      </Helmet>

      <WorkspaceHeader
        mode="buy-vs-rent"
        compareHref={compareHref}
        buyVsRentHref={workspaceBuyVsRentHref(id)}
        compareCount={compareIds.length}
        context={(
          <PlanPropertyContext
            propertyId={id}
            property={property}
            homes={homes}
            onSelect={selectProperty}
          />
        )}
        action={(
          <button
            type="button"
            className="workspace-header__edit"
            onClick={() => setAssumptionsOpen(true)}
          >
            Edit assumptions
          </button>
        )}
      />

      <div className="home-plan-body">
        <div className="home-plan-main">
          <div className="home-plan-canvas">
            <VerdictBlock
              verdict={verdict}
              action={(
                <button
                  type="button"
                  className={`home-plan-snapshot-button${snapshotSaved ? " is-saved" : ""}`}
                  onClick={toggleSnapshot}
                  aria-label={snapshotSaved ? "Remove plan snapshot" : "Save plan snapshot"}
                  title={snapshotSaved ? "Remove plan snapshot" : "Save plan snapshot"}
                >
                  <NotebookSaveIcon filled={snapshotSaved} size={17} />
                </button>
              )}
              aside={<PlanWhisper key={whisperSignature} theme={whisperTheme} />}
            />

            <div className="home-plan-assumption-summary" aria-label="Current assumptions">
              <span>₹{inputs.monthlyEmiThousands.toLocaleString("en-IN")}K EMI</span>
              <span>₹{inputs.currentRentThousands.toLocaleString("en-IN")}K rent</span>
              <span>₹{inputs.monthlySipThousands.toLocaleString("en-IN")}K SIP</span>
              <span>{inputs.loanRate.toLocaleString("en-IN")}% loan</span>
            </div>

            <section className="home-plan-stage" aria-label="Projection over time">
              <PlanGraph
                projection={projection}
                activeYear={verdict.activeYear}
                onPreviewYearChange={setPreviewYear}
                onPinYear={setPinnedYear}
              />
            </section>
          </div>
        </div>
      </div>

      <PlanAssumptionsSheet
        open={assumptionsOpen}
        inputs={inputs}
        extraEmisPerYear={extraEmisPerYear}
        onInputChange={updateInput}
        onExtraEmisChange={updateExtraEmisPerYear}
        onReset={resetInputs}
        onClose={closeAssumptions}
      />
    </div>
  );
}
