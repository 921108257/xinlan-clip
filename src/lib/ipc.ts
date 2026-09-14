/**
 * Thin typed wrapper over the Tauri command surface.
 *
 * Every backend call the UI makes goes through here so the command names and
 * payload shapes live in exactly one place.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  CaptureResult,
  Entry,
  PasteOutcome,
  PortalState,
  Settings,
} from "./types";

/** Read the clipboard into history. Returns the recorded entry, if any. */
export const captureClipboard = () =>
  invoke<CaptureResult>("capture_clipboard");

export const listEntries = (query?: string, limit = 300) =>
  invoke<Entry[]>("list_entries", { query: query ?? null, limit });

export const entryCounts = () => invoke<[number, number]>("entry_counts");

export const deleteEntry = (id: number) =>
  invoke<number>("delete_entry", { id });

export const clearHistory = (keepPinned = true) =>
  invoke<number>("clear_history", { keepPinned });

export const togglePin = (id: number) =>
  invoke<boolean | null>("toggle_pin", { id });

/**
 * Copy an entry back and, unless disabled, synthesise Ctrl+V.
 *
 * The backend hides the panel before injecting the keystroke, so callers
 * should not rely on the window still being visible afterwards.
 */
export const pasteEntry = (id: number, autoPaste?: boolean) =>
  invoke<PasteOutcome>("paste_entry", { id, autoPaste: autoPaste ?? null });

export const getSettings = () => invoke<Settings>("get_settings");

export const updateSettings = (patch: Partial<Settings>) =>
  invoke<void>("update_settings", { patch });

export const installHotkey = (binding?: string) =>
  invoke<string>("install_hotkey", { binding: binding ?? null });

export const removeHotkey = () => invoke<void>("remove_hotkey");

export const portalStatus = () => invoke<PortalState>("portal_status");

export const showSelf = () => invoke<void>("show_self");

export const hideSelf = () => invoke<void>("hide_self");

export const setCapturePaused = (paused: boolean) =>
  invoke<boolean>("set_capture_paused", { paused });

export const capturePaused = () => invoke<boolean>("capture_paused");

/** Events the backend emits; the panel reacts to all of them. */
export interface BackendEvents {
  /** The panel was just shown, so the UI can reset. */
  onPanelShown: (fn: () => void) => Promise<UnlistenFn>;
  /** The tray asked for the settings view. */
  onOpenSettings: (fn: () => void) => Promise<UnlistenFn>;
  /** Capture was paused or resumed from the tray. */
  onCapturePaused: (fn: (paused: boolean) => void) => Promise<UnlistenFn>;
  /** The synthetic-input session changed state. */
  onPortalStatus: (fn: (state: PortalState) => void) => Promise<UnlistenFn>;
}

export const events: BackendEvents = {
  onPanelShown: (fn) => listen("panel-shown", () => fn()),
  onOpenSettings: (fn) => listen("open-settings", () => fn()),
  onCapturePaused: (fn) => listen<boolean>("capture-paused", (e) => fn(e.payload)),
  onPortalStatus: (fn) =>
    listen<PortalState>("portal-status", (e) => fn(e.payload)),
};
