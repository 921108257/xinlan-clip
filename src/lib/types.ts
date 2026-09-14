/** Mirrors `Entry` in `src-tauri/src/db.rs`. */
export interface Entry {
  id: number;
  contentType: string;
  text: string;
  createdAt: number;
  lastUsedAt: number;
  useCount: number;
  pinned: boolean;
}

/** Mirrors `CaptureResult` in `src-tauri/src/commands.rs`. */
export interface CaptureResult {
  entry: Entry | null;
  trimmed: number;
}

/** Mirrors `PasteOutcome` in `src-tauri/src/commands.rs`. */
export interface PasteOutcome {
  copied: boolean;
  pasted: boolean;
  reason: string | null;
}

/**
 * Mirrors `PortalState`. Synthetic input is only available once a
 * `RemoteDesktop` session has been granted by the desktop.
 */
export type PortalState =
  | { status: "unavailable"; reason: string }
  | { status: "ready" }
  | { status: "denied"; reason: string };

/** Mirrors `Settings` in `src-tauri/src/settings.rs`. */
export interface Settings {
  maxItems: number;
  autoPaste: boolean;
  hotkeyBinding: string;
  hotkeyInstalled: boolean;
  portal: PortalState;
  autostartSupported: boolean;
}
