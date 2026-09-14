import { useCallback, useEffect, useRef, useState } from "react";

import * as ipc from "@/lib/ipc";
import type { Entry } from "@/lib/types";

/** How often the panel re-reads the clipboard while it is open. */
const POLL_INTERVAL_MS = 1000;

/** Let typing settle before hitting SQLite. */
const SEARCH_DEBOUNCE_MS = 120;

export interface UseEntriesResult {
  entries: Entry[];
  loading: boolean;
  error: string | null;
  /** Set while the clipboard is being folded into history. */
  capturing: boolean;
  refresh: () => Promise<void>;
  /** Force a clipboard read; used on show and on focus. */
  capture: () => Promise<void>;
}

/**
 * Owns the clipboard history list.
 *
 * # Why this polls
 *
 * GNOME 46 exposes no clipboard-change notification to background clients —
 * `org.freedesktop.portal.Clipboard.SelectionOwnerChanged` is defined but never
 * emitted on this build, and Wayland offers nothing else. Capturing therefore
 * has to be driven by us: once when the panel appears, and once a second while
 * it stays open, so repeated copies land in the list without reopening it.
 *
 * Polling costs a clipboard read and at most one indexed SQLite lookup per
 * second, and only while the panel is actually on screen.
 */
export function useEntries(
  search: string,
  options: { active: boolean; paused: boolean; onRecorded?: (entry: Entry) => void },
): UseEntriesResult {
  const { active, paused, onRecorded } = options;

  const [entries, setEntries] = useState<Entry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);

  const recordedRef = useRef(onRecorded);
  recordedRef.current = onRecorded;

  /** Guards against overlapping polls when a capture runs slow. */
  const inFlight = useRef(false);

  /**
   * Apply a freshly fetched list, but keep the previous array identity when the
   * rows are equivalent. That stops the polling loop from re-rendering the list
   * — and re-running entry view transitions — every second.
   */
  const commit = useCallback((rows: Entry[]) => {
    setEntries((previous) => (sameEntries(previous, rows) ? previous : rows));
  }, []);

  const refresh = useCallback(async () => {
    try {
      const rows = await ipc.listEntries(search);
      commit(rows);
      setError(null);
    } catch (cause) {
      setError(String(cause));
    }
  }, [search, commit]);

  const capture = useCallback(async () => {
    if (paused || inFlight.current) return;
    inFlight.current = true;
    setCapturing(true);
    try {
      const result = await ipc.captureClipboard();
      if (result.entry) {
        recordedRef.current?.(result.entry);
      }
      if (result.entry || result.trimmed > 0) {
        await refresh();
      }
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      inFlight.current = false;
      setCapturing(false);
    }
  }, [paused, refresh]);

  // Reload whenever the search term settles.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    const timer = setTimeout(() => {
      ipc
        .listEntries(search)
        .then((rows) => {
          if (!cancelled) {
            commit(rows);
            setError(null);
          }
        })
        .catch((cause) => {
          if (!cancelled) setError(String(cause));
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, SEARCH_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [search, commit]);

  // Capture immediately on show, then keep polling only while open.
  useEffect(() => {
    if (!active) return;

    void capture();

    const timer = setInterval(() => {
      void capture();
    }, POLL_INTERVAL_MS);

    return () => clearInterval(timer);
  }, [active, capture]);

  return { entries, loading, error, capturing, refresh, capture };
}

/** Shallow comparison of two entry lists using the fields the UI shows. */
function sameEntries(a: Entry[], b: Entry[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const x = a[i];
    const y = b[i];
    if (
      x.id !== y.id ||
      x.text !== y.text ||
      x.pinned !== y.pinned ||
      x.useCount !== y.useCount ||
      x.lastUsedAt !== y.lastUsedAt
    ) {
      return false;
    }
  }
  return true;
}
