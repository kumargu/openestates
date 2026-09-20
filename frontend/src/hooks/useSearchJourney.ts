import { useCallback, useEffect, useRef, useState } from "react";
import { useLocation, useNavigate, useSearchParams } from "react-router-dom";
import { resumeSearch, resumeSearchCheckpoint, reviseSearch, searchProperties } from "../lib/api.ts";
import { journeyUrl, readActiveJourney, readSearchCheckpoint, readSavedJourney, saveJourney, type SavedJourney } from "../lib/search-journey.ts";
import type { SearchResponse, SearchRevisionTarget } from "../lib/types.ts";
import { journeyNavigationResults } from "../lib/landing-search-rails.ts";
import { writeSearchJourneyContext } from "../lib/navigationContext.ts";
import { primaryProofFocus } from "../lib/proof-focus.ts";

export function useSearchJourney() {
  const [params] = useSearchParams();
  const { hash } = useLocation();
  const navigate = useNavigate();
  const query = params.get("q") ?? "";
  const journeyId = params.get("journey");
  const [response, setResponse] = useState<SearchResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retryKey, setRetryKey] = useState(0);
  const currentKey = useRef("");
  const controller = useRef<AbortController | null>(null);
  const pending = useRef<{ key: string; mutationId: string; utterance: string; target?: SearchRevisionTarget } | null>(null);
  const pendingRestore = useRef<SavedJourney | undefined>(undefined);
  const selectedId = useRef<string | undefined>(undefined);
  const previous = useRef<SavedJourney | undefined>(undefined);

  const accept = useCallback((result: SearchResponse, replace: boolean) => {
    const id = result.journey?.active.revision.id;
    const url = journeyUrl(query, result);
    const navigationContext = writeSearchJourneyContext(
      query, url, journeyNavigationResults(result), result.runtimeVersion,
      (card) => primaryProofFocus(card),
      Date.now(), result.journey?.active.buyerBrief ?? query,
      id,
    ) ?? undefined;
    const responseWithContext = { ...result, navigationContext };
    saveJourney(query, responseWithContext, selectedId.current, previous.current?.response.journey?.active.revision.id);
    const nextKey = JSON.stringify([query, id ?? null, new URL(url, window.location.origin).hash]);
    const sameRevision = currentKey.current === nextKey;
    currentKey.current = nextKey;
    setResponse(responseWithContext);
    setBusy(false);
    navigate(url, { replace: replace || sameRevision });
  }, [query, navigate]);

  useEffect(() => {
    const key = JSON.stringify([query, journeyId, hash]);
    if (currentKey.current === key) return;
    currentKey.current = key;
    controller.current?.abort();
    const active = new AbortController();
    controller.current = active;
    setError(null);
    pending.current = null;
    pendingRestore.current = undefined;
    const saved = readSavedJourney(journeyId);
    const checkpoint = readSearchCheckpoint(hash);
    previous.current = saved?.previousId ? readSavedJourney(saved.previousId) : undefined;
    selectedId.current = saved?.selectedId;
    setResponse(saved?.response ?? null);
    if (!query) {
      const carried = readActiveJourney();
      if (carried) navigate(journeyUrl(carried.query, carried.response), { replace: true });
      setBusy(false);
      return;
    }
    if ((journeyId && (!saved || saved.query !== query) && !checkpoint)
      || (hash.startsWith("#search=") && !checkpoint)) {
      setBusy(false);
      setError("This saved search is no longer on this device. Start a new search.");
      return;
    }
    setBusy(true);
    const request = saved && (!checkpoint || saved.response.journey?.active.revision.stateToken === checkpoint.token)
      ? resumeSearch(saved.response, { signal: active.signal })
      : checkpoint
        ? resumeSearchCheckpoint(checkpoint, { signal: active.signal })
        : searchProperties(query, { signal: active.signal });
    void request.then((result) => {
      if (!active.signal.aborted) accept(result, true);
    }).catch(() => {
      if (!active.signal.aborted) setError(checkpoint
        ? "This search could not be restored. Try again or start a new search."
        : "Search did not come through. Try again.");
    }).finally(() => { if (!active.signal.aborted) setBusy(false); });
    return () => {
      active.abort();
      // StrictMode replays effects, and Back must be able to resume this key.
      if (currentKey.current === key) currentKey.current = "";
    };
  }, [query, journeyId, hash, retryKey, accept, navigate]);

  useEffect(() => () => controller.current?.abort(), []);

  const refine = useCallback(async (utterance: string, target?: SearchRevisionTarget) => {
    if (!response?.journey || busy || !utterance.trim()) return false;
    pendingRestore.current = undefined;
    controller.current?.abort();
    const active = new AbortController();
    controller.current = active;
    setBusy(true);
    setError(null);
    const key = JSON.stringify([response.journey.active.revision.id, utterance, target]);
    if (pending.current?.key !== key) pending.current = { key, mutationId: crypto.randomUUID(), utterance, target };
    try {
      const result = await reviseSearch(response, utterance, pending.current.mutationId,
        target, selectedId.current, { signal: active.signal });
      if (active.signal.aborted) return false;
      if (result.journey?.attempt.outcome === "activated"
        && result.journey.active.revision.id !== response.journey.active.revision.id) {
        previous.current = { query, response, selectedId: selectedId.current,
          previousId: previous.current?.response.journey?.active.revision.id };
      }
      accept(result, false);
      pending.current = null;
      return true;
    } catch {
      if (!active.signal.aborted) setError("That change did not come through. Your search is unchanged; try again.");
      return false;
    } finally {
      if (!active.signal.aborted) setBusy(false);
    }
  }, [response, busy, accept, query]);

  const restore = useCallback(async (saved: SavedJourney | undefined) => {
    if (!saved || busy) return;
    pending.current = null;
    pendingRestore.current = saved;
    controller.current?.abort();
    const active = new AbortController();
    controller.current = active;
    setBusy(true);
    setError(null);
    try {
      const result = await resumeSearch(saved.response, { signal: active.signal });
      if (active.signal.aborted) return;
      previous.current = saved.previousId ? readSavedJourney(saved.previousId) : undefined;
      selectedId.current = saved.selectedId;
      accept(result, false);
      pendingRestore.current = undefined;
    } catch {
      if (!active.signal.aborted) setError("The previous search could not be restored. Your search is unchanged.");
    } finally {
      if (!active.signal.aborted) setBusy(false);
    }
  }, [busy, accept]);

  const undo = useCallback(() => restore(previous.current), [restore]);

  const retry = useCallback(() => {
    if (pendingRestore.current) {
      void restore(pendingRestore.current);
      return;
    }
    if (pending.current) {
      void refine(pending.current.utterance, pending.current.target);
      return;
    }
    currentKey.current = "";
    setRetryKey((value) => value + 1);
  }, [refine, restore]);
  return { response, busy, error, refine, retry, undo, restore, canUndo: Boolean(previous.current) };
}
