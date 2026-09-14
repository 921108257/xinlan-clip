import {
  useCallback,
  useEffect,
  useRef,
  useState,
  startTransition,
  ViewTransition,
} from "react";

import { EntryList } from "@/components/EntryList";
import { HintBanner } from "@/components/HintBanner";
import { PanelFooter } from "@/components/PanelFooter";
import { PanelHeader } from "@/components/PanelHeader";
import { SettingsView } from "@/components/SettingsView";
import { useEntries } from "@/hooks/useEntries";
import { useKeyboardNav } from "@/hooks/useKeyboardNav";
import * as ipc from "@/lib/ipc";
import type { Entry, PortalState, Settings } from "@/lib/types";

/** How long a transient status message stays on screen. */
const TOAST_MS = 2600;

type View = "list" | "settings";

export default function App() {
  const [search, setSearch] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [view, setView] = useState<View>("list");
  const [paused, setPaused] = useState(false);
  const [counts, setCounts] = useState<[number, number]>([0, 0]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [portal, setPortal] = useState<PortalState>({
    status: "unavailable",
    reason: "启动中",
  });
  const [toast, setToast] = useState<string | null>(null);

  const searchRef = useRef<HTMLInputElement>(null);
  const toastTimer = useRef<number | null>(null);
  /** Message to surface the next time the panel is looked at. */
  const pendingToast = useRef<string | null>(null);

  const notify = useCallback((message: string) => {
    setToast(message);
    if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), TOAST_MS);
  }, []);

  const refreshCounts = useCallback(async () => {
    try {
      setCounts(await ipc.entryCounts());
    } catch {
      // A missing database just means there is no history yet.
    }
  }, []);

  const loadSettings = useCallback(async () => {
    try {
      const next = await ipc.getSettings();
      setSettings(next);
      setPortal(next.portal);
    } catch (cause) {
      notify(`读取设置失败：${cause}`);
    }
  }, [notify]);

  const { entries, loading, error, capturing, refresh, capture } = useEntries(
    search,
    {
      // Only poll while the panel is actually on screen.
      active: view === "list" && !paused,
      paused,
      onRecorded: () => {
        if (search.trim() === "") setActiveIndex(0);
      },
    },
  );

  // Keep the cursor inside the list as it changes underneath us.
  useEffect(() => {
    if (activeIndex > entries.length - 1) {
      setActiveIndex(Math.max(0, entries.length - 1));
    }
  }, [entries.length, activeIndex]);

  useEffect(() => {
    void refreshCounts();
  }, [refreshCounts, entries]);

  useEffect(() => {
    void loadSettings();
    void ipc
      .capturePaused()
      .then(setPaused)
      .catch(() => undefined);
  }, [loadSettings]);

  const hide = useCallback(() => {
    void ipc.hideSelf();
  }, []);

  // ------------------------------------------------------------------ actions

  const handleSelect = useCallback(
    async (index: number) => {
      const entry = entries[index];
      if (!entry) return;

      try {
        const outcome = await ipc.pasteEntry(entry.id);
        if (outcome.pasted) {
          // The portal accepted the keystroke. Whether it actually reaches the
          // focused window is the desktop's call — GNOME 46 accepts these calls
          // but does not deliver them — so queue a note rather than claiming a
          // definite success. It shows when the user looks at the panel again.
          pendingToast.current = "已复制到剪贴板";
          return;
        }
        notify(outcome.reason ?? "已复制到剪贴板");
        void refresh();
        void refreshCounts();
      } catch (cause) {
        notify(`粘贴失败：${cause}`);
      }
    },
    [entries, notify, refresh, refreshCounts],
  );

  useKeyboardNav({
    itemCount: entries.length,
    activeIndex,
    setActiveIndex,
    onSelect: (index) => void handleSelect(index),
    onEscape: hide,
    // While the settings view is up the list shortcuts should not fire.
    disabled: view !== "list",
  });

  const handleTogglePin = useCallback(
    async (entry: Entry) => {
      try {
        await ipc.togglePin(entry.id);
        startTransition(() => {
          void refresh();
        });
      } catch (cause) {
        notify(`操作失败：${cause}`);
      }
    },
    [notify, refresh],
  );

  const handleDelete = useCallback(
    async (entry: Entry) => {
      try {
        await ipc.deleteEntry(entry.id);
        startTransition(() => {
          void refresh();
        });
        void refreshCounts();
      } catch (cause) {
        notify(`删除失败：${cause}`);
      }
    },
    [notify, refresh, refreshCounts],
  );

  const handleClear = useCallback(
    async (keepPinned: boolean) => {
      try {
        const removed = await ipc.clearHistory(keepPinned);
        startTransition(() => {
          setActiveIndex(0);
          void refresh();
        });
        void refreshCounts();
        notify(`已删除 ${removed} 条记录`);
      } catch (cause) {
        notify(`清空失败：${cause}`);
      }
    },
    [notify, refresh, refreshCounts],
  );

  const handleTogglePause = useCallback(
    async (next: boolean) => {
      try {
        setPaused(await ipc.setCapturePaused(next));
      } catch (cause) {
        notify(`操作失败：${cause}`);
      }
    },
    [notify],
  );

  // ------------------------------------------------------------------- events

  useEffect(() => {
    const disposers: Array<() => void> = [];
    let cancelled = false;

    const register = async () => {
      const [offShown, offSettings, offPaused, offPortal] = await Promise.all([
        ipc.events.onPanelShown(() => {
          const queued = pendingToast.current;
          pendingToast.current = null;

          // A fresh interaction: clear the filter and return to the list.
          startTransition(() => {
            setSearch("");
            setActiveIndex(0);
            setView("list");
            setToast(queued);
          });
          if (queued) {
            if (toastTimer.current !== null) window.clearTimeout(toastTimer.current);
            toastTimer.current = window.setTimeout(() => setToast(null), TOAST_MS);
          }

          void capture();
          searchRef.current?.focus();
        }),
        ipc.events.onOpenSettings(() => {
          startTransition(() => setView("settings"));
          void loadSettings();
        }),
        ipc.events.onCapturePaused((next) => setPaused(next)),
        ipc.events.onPortalStatus((next) => {
          setPortal(next);
          setSettings((previous) =>
            previous ? { ...previous, portal: next } : previous,
          );
        }),
      ]);

      if (cancelled) {
        offShown();
        offSettings();
        offPaused();
        offPortal();
        return;
      }
      disposers.push(offShown, offSettings, offPaused, offPortal);
    };

    void register();
    return () => {
      cancelled = true;
      disposers.forEach((dispose) => dispose());
    };
  }, [capture, loadSettings]);

  // Regaining focus is the strongest hint that the clipboard may have changed.
  useEffect(() => {
    const onFocus = () => {
      void capture();
      searchRef.current?.focus();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [capture]);

  // Park the cursor in the search box as soon as the panel is interactive.
  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  const [, pinned] = counts;

  return (
    <div className="relative flex h-full flex-col">
        <PanelHeader
          search={search}
          onSearchChange={(value) => {
            startTransition(() => {
              setSearch(value);
              setActiveIndex(0);
            });
          }}
          searchRef={searchRef}
          onOpenSettings={() => {
            startTransition(() => setView("settings"));
            void loadSettings();
          }}
          onClose={hide}
          capturing={capturing}
        />

        <HintBanner
          portal={portal}
          hotkeyInstalled={settings?.hotkeyInstalled ?? true}
        />

        {error && (
          <p
            role="alert"
            className="border-b border-destructive/30 bg-destructive/10 px-3 py-1.5 text-[0.7rem] text-destructive"
          >
            {error}
          </p>
        )}

        {/*
          The list and the settings sheet swap in place, so they carry the enter
          and exit transition. `default="none"` keeps every other update inside
          the subtree silent — only this state change animates.
        */}
        {view === "list" ? (
          <ViewTransition
            key="list"
            default="none"
            enter="vt-slide-up"
            exit="vt-fade-out"
          >
            <EntryList
              entries={entries}
              activeIndex={activeIndex}
              loading={loading}
              search={search}
              onActivate={setActiveIndex}
              onSelect={(index) => void handleSelect(index)}
              onTogglePin={handleTogglePin}
              onDelete={handleDelete}
            />
          </ViewTransition>
        ) : (
          <ViewTransition
            key="settings"
            default="none"
            enter="vt-slide-up"
            exit="vt-fade-out"
          >
            <SettingsView
              settings={settings}
              onBack={() => startTransition(() => setView("list"))}
              onChanged={() => {
                void loadSettings();
                void refreshCounts();
              }}
            />
          </ViewTransition>
        )}

        <PanelFooter
          total={counts[0]}
          pinned={pinned}
          paused={paused}
          capturing={capturing}
          canClear={entries.length > 0}
          onClear={handleClear}
          onTogglePause={handleTogglePause}
        />

        {/*
          Transient feedback for the copy-only fallback, announced to screen
          readers as well as shown visually.
        */}
        <div
          aria-live="polite"
          className="pointer-events-none absolute inset-x-0 bottom-12 flex justify-center"
        >
          {toast && (
            <ViewTransition default="none" enter="vt-slide-up" exit="vt-fade-out">
              <p className="pointer-events-auto max-w-[85%] rounded-full border border-border bg-popover/95 px-3 py-1.5 text-center text-[0.7rem] shadow-lg backdrop-blur">
                {toast}
              </p>
            </ViewTransition>
          )}
        </div>
    </div>
  );
}
