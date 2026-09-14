//! xinlan-clip — clipboard history for Linux.
//!
//! # Shape of the application
//!
//! The window is a frameless panel that lives in the tray. It is shown by the
//! tray menu, by a second launch (`xinlan-clip --toggle`), or by the GNOME
//! custom shortcut that runs exactly that command.
//!
//! # Platform constraints that shaped the design
//!
//! Both of these were measured on the target machine (Ubuntu 24.04, GNOME 46,
//! Wayland) rather than assumed:
//!
//! 1. **No clipboard-change notification exists for background clients.**
//!    `org.freedesktop.portal.Clipboard.SelectionOwnerChanged` is present in the
//!    interface but never emitted here, and Wayland gives no other hook. History
//!    is therefore captured on demand — when the panel is shown, and once a
//!    second while it remains open.
//! 2. **The app cannot register its own global hotkey.** GNOME 46 does not
//!    implement `org.freedesktop.portal.GlobalShortcuts`, and the X11 grab path
//!    used by Tauri's global-shortcut plugin does not apply to a Wayland client.
//!    The user's own keybinding is used instead (see `gnome_hotkey`).
//!
//! Synthetic Ctrl+V *is* available, via the `RemoteDesktop` portal — see
//! `portal.rs`.

mod clipboard;
mod commands;
mod db;
mod gnome_hotkey;
mod portal;
mod settings;

use std::sync::atomic::Ordering;

use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use commands::{AppState, WINDOW_LABEL};
use portal::{Portal, PortalState};

/// Accelerator offered when registering the GNOME shortcut.
///
/// `Super+V` would be closer to Windows, but GNOME 46 already owns it, so the
/// default stays out of the shell's way.
pub const DEFAULT_HOTKEY: &str = "<Control><Alt>v";

/// Location of the clipboard history database.
///
/// Resolved through Tauri rather than hard-coded so the platform convention is
/// respected: `~/.local/share/com.xinlan.clip/clipboard.db` on Linux.
pub fn database_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("clipboard.db"))
        .map_err(|e| format!("cannot resolve the data directory: {e}"))
}

/// Raise the panel, move it to the foreground, and hand focus to the search box.
pub fn show_panel(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW_LABEL) else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();

    // The WebView keeps its scroll position and selection between shows, so
    // tell the frontend to reset itself for a fresh interaction.
    let _ = app.emit("panel-shown", ());
}

/// Wire up the tray icon and its menu.
fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "显示剪贴板面板").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "设置…").build(app)?;
    let paused = CheckMenuItemBuilder::with_id("pause", "暂停记录")
        .checked(false)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&settings)
        .separator()
        .item(&paused)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::AssetNotFound("default window icon missing".into())
        })?)
        .tooltip("xinlan-clip — 剪贴板历史")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => show_panel(app),
            "settings" => {
                show_panel(app);
                let _ = app.emit("open-settings", ());
            }
            "pause" => {
                let state = app.state::<AppState>();
                let next = !state.capture_paused.load(Ordering::SeqCst);
                state.capture_paused.store(next, Ordering::SeqCst);
                let _ = app.emit("capture-paused", next);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click toggles the panel; the menu covers the rest.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_panel(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Bring up the synthetic-input session in the background.
///
/// Deliberately non-blocking: the panel must be usable even if the portal is
/// refused, in which case pasting degrades to copy-only.
fn start_portal_session(app: &AppHandle) {
    let app = app.clone();
    // The portal client blocks on a D-Bus conversation, so it runs on the
    // blocking pool rather than occupying an async worker.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        match Portal::connect() {
            Ok(session) => {
                if let Ok(mut guard) = state.session.lock() {
                    *guard = Some(session);
                }
                state.set_portal_state(PortalState::Ready);
                let _ = app.emit("portal-status", PortalState::Ready);
            }
            Err(reason) => {
                let denied = reason.starts_with("cancelled") || reason.starts_with("denied");
                let next = if denied {
                    PortalState::Denied { reason }
                } else {
                    PortalState::Unavailable { reason }
                };
                state.set_portal_state(next.clone());
                let _ = app.emit("portal-status", next);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[cfg(not(test))]
pub fn run() {
    // A second launch (which is what the GNOME shortcut does) should raise the
    // panel in the process that is already running, not start a rival copy.
    // `--toggle` is the flag the registered shortcut passes; a plain re-launch
    // from the application menu is treated the same way.
    let single_instance = tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        show_panel(app);
    });

    tauri::Builder::default()
        .plugin(single_instance)
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::capture_clipboard,
            commands::set_capture_paused,
            commands::capture_paused,
            commands::list_entries,
            commands::entry_counts,
            commands::delete_entry,
            commands::clear_history,
            commands::toggle_pin,
            commands::paste_entry,
            commands::get_settings,
            commands::update_settings,
            commands::portal_status,
            commands::install_hotkey,
            commands::remove_hotkey,
            commands::hotkey_status,
            commands::show_self,
            commands::hide_self,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            build_tray(&handle)?;

            // Open the history database up front so the file exists and the
            // schema is applied before the panel can issue its first command.
            match database_path(&handle) {
                Ok(path) => match tauri::async_runtime::block_on(db::connect(&path)) {
                    Ok(pool) => handle.state::<AppState>().set_pool(pool),
                    Err(error) => {
                        eprintln!("xinlan-clip: cannot open {}: {error}", path.display());
                    }
                },
                Err(error) => eprintln!("xinlan-clip: {error}"),
            }

            // Hiding on focus loss is what makes the panel feel like a
            // popover. It is suppressed while a paste is in flight, because
            // that path hides the window on purpose.
            if let Some(window) = handle.get_webview_window(WINDOW_LABEL) {
                let handle = handle.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::Focused(false) = event {
                        let state = handle.state::<AppState>();
                        if state.paste_in_flight.load(Ordering::SeqCst) {
                            return;
                        }
                        if let Some(window) = handle.get_webview_window(WINDOW_LABEL) {
                            let _ = window.hide();
                        }
                    }
                });
            }

            start_portal_session(&handle);

            // GNOME has no built-in clipboard history before version 48, and
            // the tray is easy to miss, so show the panel on first launch.
            show_panel(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running xinlan-clip");
}
