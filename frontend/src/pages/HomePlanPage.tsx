import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { Helmet } from "react-helmet-async";
import { Link, useNavigate, useParams } from "react-router-dom";
import { getProperties, getProperty } from "../lib/api.ts";
import type { PropertyCard, PropertyDetailResponse } from "../lib/types.ts";
import { WorkspaceHeader } from "../components/workspace/WorkspaceHeader.tsx";
import { useNotebook } from "../hooks/useNotebook.ts";
import {
  workspaceBuyVsRentHref,
  workspaceCompareHref,
  workspacePlanReplacementId,
} from "../lib/workspaceNav.ts";
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
import {
  canPersistPlanDraft,
  readPlanDraft,
  writePlanDraft,
} from "../features/home-plan/planDrafts.ts";

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

type PlanHomeOption = {
  id: string;
  label: string;
  meta: string;
};

function planHomeMeta(bhk: number, sqft: number, price: number): string {
  return `${bhk} BHK · ${sqft.toLocaleString("en-IN")} sqft · ${formatCurrency(price, true)}`;
}

function PlanPropertyContext({
  propertyId,
  homes,
  onSelect,
}: {
  propertyId?: string;
  homes: PlanHomeOption[];
  onSelect: (propertyId: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const contextRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const listboxId = useId();
  const selectedHome = homes.find((home) => home.id === propertyId) ?? homes[0];

  useEffect(() => {
    if (!open) return undefined;
    const handlePointerDown = (event: PointerEvent) => {
      if (!contextRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      triggerRef.current?.focus();
    };

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  if (!selectedHome) return null;

  if (homes.length === 1) {
    return (
      <div className="workspace-plan-context workspace-plan-context--single">
        <strong>{selectedHome.label}</strong>
        <span>{selectedHome.meta}</span>
      </div>
    );
  }

  const handleListKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const options = [...(contextRef.current
      ?.querySelectorAll<HTMLButtonElement>(".workspace-plan-context__option") ?? [])];
    if (options.length === 0) return;
    event.preventDefault();
    const activeIndex = options.findIndex((option) => option === document.activeElement);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? options.length - 1
        : event.key === "ArrowUp"
          ? (activeIndex - 1 + options.length) % options.length
          : (activeIndex + 1) % options.length;
    options[nextIndex]?.focus();
  };

  return (
    <div ref={contextRef} className="workspace-plan-context">
      <button
        ref={triggerRef}
        type="button"
        className="workspace-plan-context__trigger"
        aria-label={`Switch home, currently ${selectedHome.label}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (!["ArrowDown", "ArrowUp"].includes(event.key)) return;
          event.preventDefault();
          if (!open) setOpen(true);
          requestAnimationFrame(() => {
            contextRef.current
              ?.querySelector<HTMLButtonElement>('[role="option"][aria-selected="true"]')
              ?.focus();
          });
        }}
      >
        <span className="workspace-plan-context__identity">
          <strong>{selectedHome.label}</strong>
          <span>{selectedHome.meta}</span>
        </span>
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <path d="m4 6 4 4 4-4" />
        </svg>
      </button>
      {open && (
        <div
          id={listboxId}
          className="workspace-plan-context__menu"
          role="listbox"
          aria-label="Homes for Buy vs Rent"
          onKeyDown={handleListKeyDown}
        >
          {homes.map((home) => {
            const selected = home.id === selectedHome.id;
            return (
              <button
                key={home.id}
                type="button"
                className="workspace-plan-context__option"
                role="option"
                aria-selected={selected}
                tabIndex={selected ? 0 : -1}
                onClick={() => {
                  setOpen(false);
                  onSelect(home.id);
                }}
              >
                <span>
                  <strong>{home.label}</strong>
                  <small>{home.meta}</small>
                </span>
                <span className="workspace-plan-context__check" aria-hidden="true">
                  {selected ? "✓" : ""}
                </span>
              </button>
            );
          })}
        </div>
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
  const { compareIds, propertyIds } = useNotebook();
  const [catalog, setCatalog] = useState<PropertyCard[]>([]);
  const [catalogReady, setCatalogReady] = useState(false);
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
    let active = true;
    getProperties({ signal: controller.signal })
      .then((homes) => {
        if (active) setCatalog(homes);
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === "AbortError") return;
        if (active) setCatalog([]);
      })
      .finally(() => {
        if (active) setCatalogReady(true);
      });
    return () => {
      active = false;
      controller.abort();
    };
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
    if (!id || !canPersistPlanDraft(id, propertyData?.property.id, status) || !inputs) return;
    writePlanDraft(id, inputs, extraEmisPerYear);
  }, [extraEmisPerYear, id, inputs, propertyData?.property.id, status]);

  const projection = useMemo(
    () => inputs ? calculateProjection(inputs, extraEmisPerYear) : null,
    [inputs, extraEmisPerYear],
  );

  const workspacePropertyIds = [...new Set([...(id ? [id] : []), ...propertyIds])];
  const homeOptions = workspacePropertyIds.flatMap((propertyId) => {
    if (status === "not_found" && propertyId === id) return [];
    const catalogHome = catalog.find((home) => home.id === propertyId);
    if (catalogHome) {
      return [{
        id: propertyId,
        label: displayName(catalogHome),
        meta: planHomeMeta(catalogHome.bhk, catalogHome.sqft, catalogHome.price),
      }];
    }
    if (propertyData?.property.id === propertyId) {
      return [{
        id: propertyId,
        label: propertyData.society?.name?.trim() || propertyData.property.title,
        meta: planHomeMeta(
          propertyData.property.bhk,
          propertyData.property.super_builtup_sqft,
          propertyData.property.price,
        ),
      }];
    }
    return [];
  });
  const compareHref = workspaceCompareHref(compareIds, id);
  const buyVsRentHref = workspaceBuyVsRentHref(id ?? propertyIds[0]);
  const planReplacementId = catalogReady && (!id || status === "not_found")
    ? workspacePlanReplacementId(id, homeOptions.map((home) => home.id))
    : null;

  useEffect(() => {
    if (!planReplacementId) return;
    navigate(workspaceBuyVsRentHref(planReplacementId), { replace: true });
  }, [navigate, planReplacementId]);

  const selectProperty = (propertyId: string) => {
    if (propertyId) navigate(workspaceBuyVsRentHref(propertyId));
  };

  if (
    !id
    || status !== "ready"
    || propertyData?.property.id !== id
    || !inputs
    || !projection
  ) {
    const propertyIsChanging = Boolean(id)
      && status === "ready"
      && propertyData?.property.id !== id;
    const content = planReplacementId || (!catalogReady && !id) ? (
      <LoadingPlan />
    ) : !id ? (
      <section className="home-plan-empty">
        <h1>Choose a home to plan.</h1>
        <p>Buy vs Rent uses the price and status of one home from your workspace.</p>
        <Link to="/">Discover homes</Link>
      </section>
    ) : status === "loading" || propertyIsChanging ? (
      <LoadingPlan />
    ) : status === "not_found" ? (
      <section className="home-plan-empty">
        <h1>This home is no longer available.</h1>
        <p>Add another home to your workspace and its Buy vs Rent plan will be ready here.</p>
        <Link to="/">Explore homes</Link>
      </section>
    ) : (
      <section className="home-plan-empty">
        <h1>We couldn’t open this plan.</h1>
        <p>Try again in a moment, or continue with another home in your workspace.</p>
        <Link to="/workspace">Back to workspace</Link>
      </section>
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
          context={homeOptions.length > 0 ? (
            <PlanPropertyContext
              propertyId={homeOptions.some((home) => home.id === id) ? id : undefined}
              homes={homeOptions}
              onSelect={selectProperty}
            />
          ) : undefined}
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
            homes={homeOptions}
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
