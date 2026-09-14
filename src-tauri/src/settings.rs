//! User-facing settings, persisted in the `settings` table.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db;

pub const KEY_MAX_ITEMS: &str = "max_items";
pub const KEY_AUTO_PASTE: &str = "auto_paste";
pub const KEY_HOTKEY: &str = "hotkey_binding";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Maximum number of unpinned entries retained.
    pub max_items: i64,
    /// Whether selecting an entry also synthesises Ctrl+V.
    pub auto_paste: bool,
    /// Accelerator written into the GNOME custom shortcut.
    pub hotkey_binding: String,
    /// Whether the GNOME shortcut slot is currently registered.
    pub hotkey_installed: bool,
    /// Whether the synthetic-input session is live.
    pub portal: crate::portal::PortalState,
    /// Always false on Linux; kept so the UI can hide the option.
    pub autostart_supported: bool,
}

impl Settings {
    pub async fn load(pool: &SqlitePool) -> Self {
        let max_items = db::max_items(pool).await;
        let auto_paste = db::auto_paste(pool).await;
        let hotkey_binding = db::get_setting(pool, KEY_HOTKEY, crate::DEFAULT_HOTKEY).await;
        let hotkey_installed = crate::gnome_hotkey::status().unwrap_or(false);

        Self {
            max_items,
            auto_paste,
            hotkey_binding,
            hotkey_installed,
            portal: crate::portal::PortalState::Unavailable {
                reason: "initialising".into(),
            },
            autostart_supported: false,
        }
    }
}

/// The fields a caller is allowed to change.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub max_items: Option<i64>,
    pub auto_paste: Option<bool>,
    pub hotkey_binding: Option<String>,
}

impl SettingsPatch {
    pub async fn apply(&self, pool: &SqlitePool) -> Result<(), String> {
        if let Some(value) = self.max_items {
            let clamped = value.clamp(10, 10_000);
            db::set_setting(pool, KEY_MAX_ITEMS, &clamped.to_string()).await?;
            db::trim(pool, clamped).await?;
        }
        if let Some(value) = self.auto_paste {
            db::set_setting(pool, KEY_AUTO_PASTE, if value { "true" } else { "false" }).await?;
        }
        if let Some(value) = self.hotkey_binding.as_deref() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                db::set_setting(pool, KEY_HOTKEY, trimmed).await?;
                // Re-register so GNOME picks the new accelerator up.
                if crate::gnome_hotkey::status().unwrap_or(false) {
                    let exe = crate::gnome_hotkey::current_exe()?;
                    crate::gnome_hotkey::install(&exe, Some(trimmed), crate::DEFAULT_HOTKEY)?;
                }
            }
        }
        Ok(())
    }
}
