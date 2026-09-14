//! Tauri commands: the entire surface the frontend is allowed to call.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};

use crate::clipboard;
use crate::db::{self, Entry};
use crate::portal::{Portal, PortalState};
use crate::settings::{Settings, SettingsPatch};
use crate::{gnome_hotkey, show_panel};

/// How long to let the previously focused window reclaim focus after the panel
/// hides. Too short and Ctrl+V lands in nothing; too long and it feels laggy.
const FOCUS_HANDOFF: std::time::Duration = std::time::Duration::from_millis(160);

pub const WINDOW_LABEL: &str = "main";

/// Process-wide state shared by the commands.
pub struct AppState {
    /// Clipboard history pool, filled in during setup.
    pub db: Mutex<Option<SqlitePool>>,
    /// Guarantees the pool is opened exactly once even if the panel is quick.
    pub db_init: tokio::sync::Mutex<()>,
    /// Live synthetic-input session, or the reason there isn't one.
    pub portal: Mutex<PortalState>,
    /// Kept alive for the whole process so the portal grant survives.
    pub session: Mutex<Option<Portal>>,
    /// Set while the panel hides itself, so the focus handler stays quiet.
    pub paste_in_flight: AtomicBool,
    /// User-controlled pause switch.
    pub capture_paused: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: Mutex::new(None),
            db_init: tokio::sync::Mutex::new(()),
            portal: Mutex::new(PortalState::Unavailable {
                reason: "starting".into(),
            }),
            session: Mutex::new(None),
            paste_in_flight: AtomicBool::new(false),
            capture_paused: AtomicBool::new(false),
        }
    }

    /// Hand over an already-opened pool.
    pub fn set_pool(&self, pool: SqlitePool) {
        if let Ok(mut guard) = self.db.lock() {
            *guard = Some(pool);
        }
    }

    /// Return the pool, opening it if setup has not finished yet.
    ///
    /// The panel can issue its first capture before `setup` completes, so this
    /// cannot assume the pool is present. Creation happens under a dedicated
    /// lock so two concurrent callers cannot race.
    pub async fn pool(&self, path: &std::path::Path) -> Result<SqlitePool, String> {
        if let Some(pool) = self.cached_pool() {
            return Ok(pool);
        }

        let _init = self.db_init.lock().await;

        // Another caller may have finished while we waited.
        if let Some(pool) = self.cached_pool() {
            return Ok(pool);
        }

        let pool = db::connect(path).await?;
        self.set_pool(pool.clone());
        Ok(pool)
    }

    fn cached_pool(&self) -> Option<SqlitePool> {
        self.db.lock().ok().and_then(|guard| guard.clone())
    }

    pub fn portal_state(&self) -> PortalState {
        self.portal
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or(PortalState::Unavailable {
                reason: "state poisoned".into(),
            })
    }

    pub fn set_portal_state(&self, next: PortalState) {
        if let Ok(mut guard) = self.portal.lock() {
            *guard = next;
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve the history pool for a command.
async fn pool(app: &AppHandle, state: &AppState) -> Result<SqlitePool, String> {
    state.pool(&crate::database_path(app)?).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    /// Present when the clipboard held text we had not already recorded.
    pub entry: Option<Entry>,
    /// Number of entries removed by the retention pass.
    pub trimmed: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasteOutcome {
    /// The entry text is on the clipboard.
    pub copied: bool,
    /// Ctrl+V was synthesised into the focused window.
    pub pasted: bool,
    /// Why the synthetic paste did not happen, when it did not.
    pub reason: Option<String>,
}

// ------------------------------------------------------------------ capturing

/// Read the clipboard and fold it into the history.
///
/// This is the only capture path: GNOME 46 offers no clipboard-change
/// notification to background clients, so the panel calls this on show, on
/// focus, and once a second while it is open.
#[tauri::command]
pub async fn capture_clipboard(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CaptureResult, String> {
    let empty = CaptureResult {
        entry: None,
        trimmed: 0,
    };

    if state.capture_paused.load(Ordering::SeqCst) {
        return Ok(empty);
    }

    let Some(text) = clipboard::read_text()? else {
        return Ok(empty);
    };
    if text.is_empty() {
        return Ok(empty);
    }

    // Our own paste echo is not new information.
    if clipboard::is_self_written(clipboard::hash_u64(&text)) {
        return Ok(empty);
    }

    let pool = pool(&app, &state).await?;
    let hash = clipboard::hash_text(&text);
    let now = db::now_unix();

    let entry = db::upsert_entry(&pool, &text, &hash, now).await?;
    let trimmed = db::trim(&pool, db::max_items(&pool).await).await?;

    Ok(CaptureResult { entry, trimmed })
}

/// Pause or resume history capture (tray toggle).
#[tauri::command]
pub fn set_capture_paused(state: State<'_, AppState>, paused: bool) -> bool {
    state.capture_paused.store(paused, Ordering::SeqCst);
    paused
}

#[tauri::command]
pub fn capture_paused(state: State<'_, AppState>) -> bool {
    state.capture_paused.load(Ordering::SeqCst)
}

// ------------------------------------------------------------------- reading

#[tauri::command]
pub async fn list_entries(
    app: AppHandle,
    state: State<'_, AppState>,
    query: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<Entry>, String> {
    let pool = pool(&app, &state).await?;
    db::list_entries(&pool, query.as_deref(), limit.unwrap_or(200)).await
}

#[tauri::command]
pub async fn entry_counts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(i64, i64), String> {
    let pool = pool(&app, &state).await?;
    db::counts(&pool).await
}

// ------------------------------------------------------------------- mutating

#[tauri::command]
pub async fn delete_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<u64, String> {
    let pool = pool(&app, &state).await?;
    db::delete_entry(&pool, id).await
}

#[tauri::command]
pub async fn clear_history(
    app: AppHandle,
    state: State<'_, AppState>,
    keep_pinned: Option<bool>,
) -> Result<u64, String> {
    let pool = pool(&app, &state).await?;
    db::clear_history(&pool, keep_pinned.unwrap_or(true)).await
}

#[tauri::command]
pub async fn toggle_pin(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<bool>, String> {
    let pool = pool(&app, &state).await?;
    db::toggle_pin(&pool, id).await
}

// --------------------------------------------------------------------- pasting

/// Copy an entry back to the clipboard and, when asked, paste it.
///
/// Ordering matters: the panel must give up focus *before* the keystroke is
/// synthesised, otherwise Ctrl+V is delivered to our own webview.
#[tauri::command]
pub async fn paste_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    auto_paste: Option<bool>,
) -> Result<PasteOutcome, String> {
    let pool = pool(&app, &state).await?;

    let entry = db::get_entry(&pool, id)
        .await?
        .ok_or_else(|| "该记录已不存在".to_string())?;

    clipboard::write_text(&entry.text)?;
    db::mark_used(&pool, id, db::now_unix()).await?;

    let wants_paste = auto_paste.unwrap_or(db::auto_paste(&pool).await);
    if !wants_paste {
        return Ok(PasteOutcome {
            copied: true,
            pasted: false,
            reason: Some("自动粘贴已关闭".into()),
        });
    }

    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        state.paste_in_flight.store(true, Ordering::SeqCst);
        let _ = window.hide();
    }

    // Let focus settle on whichever window the user came from.
    tokio::time::sleep(FOCUS_HANDOFF).await;

    // Cloning the handle out keeps the mutex away from the blocking section.
    let session = state.session.lock().ok().and_then(|guard| guard.clone());

    state.paste_in_flight.store(false, Ordering::SeqCst);

    let Some(portal) = session else {
        return Ok(PasteOutcome {
            copied: true,
            pasted: false,
            reason: Some("未获得输入权限，内容已复制".into()),
        });
    };

    // The portal client is blocking, so keep it off the async worker.
    let outcome = tauri::async_runtime::spawn_blocking(move || portal.paste())
        .await
        .map_err(|e| format!("粘贴任务失败：{e}"))?;

    match outcome {
        Ok(()) => Ok(PasteOutcome {
            copied: true,
            pasted: true,
            reason: None,
        }),
        Err(error) => Ok(PasteOutcome {
            copied: true,
            pasted: false,
            reason: Some(format!("模拟按键失败（内容已复制）：{error}")),
        }),
    }
}

// -------------------------------------------------------------------- settings

#[tauri::command]
pub async fn get_settings(app: AppHandle, state: State<'_, AppState>) -> Result<Settings, String> {
    let pool = pool(&app, &state).await?;
    let mut settings = Settings::load(&pool).await;
    settings.portal = state.portal_state();
    Ok(settings)
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<(), String> {
    let pool = pool(&app, &state).await?;
    patch.apply(&pool).await
}

#[tauri::command]
pub fn portal_status(state: State<'_, AppState>) -> PortalState {
    state.portal_state()
}

// --------------------------------------------------------------- os integration

#[tauri::command]
pub async fn install_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    binding: Option<String>,
) -> Result<String, String> {
    // Registering the shortcut must not depend on the history database being
    // reachable, so a database failure here still lets the binding succeed.
    let stored = pool(&app, &state).await.ok();

    let exe = gnome_hotkey::current_exe()?;
    let binding = match binding {
        Some(value) if !value.trim().is_empty() => value,
        _ => match stored.as_ref() {
            Some(pool) => {
                db::get_setting(pool, crate::settings::KEY_HOTKEY, crate::DEFAULT_HOTKEY).await
            }
            None => crate::DEFAULT_HOTKEY.to_string(),
        },
    };

    let applied = gnome_hotkey::install(&exe, Some(&binding), crate::DEFAULT_HOTKEY)?;
    if let Some(pool) = stored.as_ref() {
        let _ = db::set_setting(pool, crate::settings::KEY_HOTKEY, &applied).await;
    }
    Ok(applied)
}

#[tauri::command]
pub fn remove_hotkey() -> Result<(), String> {
    gnome_hotkey::uninstall()
}

#[tauri::command]
pub fn hotkey_status() -> bool {
    gnome_hotkey::status().unwrap_or(false)
}

// ------------------------------------------------------------------ window glue

#[tauri::command]
pub fn show_self(app: AppHandle) {
    show_panel(&app);
}

#[tauri::command]
pub fn hide_self(app: AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.hide();
    }
}
