//! GNOME custom keyboard shortcut management.
//!
//! # Why the app cannot do this itself
//!
//! Registering a global hotkey normally means `XGrabKey` (X11) or
//! `org.freedesktop.portal.GlobalShortcuts` (Wayland). GNOME 46 implements
//! neither for plain clients: the portal interface is absent from the running
//! `xdg-desktop-portal`, and the `global-hotkey` crate used by Tauri's
//! global-shortcut plugin is X11-only, so it cannot work on a Wayland session.
//!
//! The window manager does own the keyboard, however, and it lets the *user*
//! bind any command to a key. So instead of grabbing the key we register a
//! shortcut that runs `xinlan-clip --toggle`. The single-instance plugin
//! forwards that to the running process, which raises the panel — the same
//! arrangement other GNOME clipboard managers use.
//!
//! Bindings are written to the same schema the GNOME Settings UI edits, so the
//! shortcut shows up (and can be changed) under "Custom Shortcuts".

use std::process::Command;

const MEDIA_KEYS_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CUSTOM_SCHEMA_PREFIX: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const BINDING_BASE: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";
const SLOT: &str = "xinlan-clip";
const SHORTCUT_NAME: &str = "xinlan-clip 剪贴板历史";

fn slot_path() -> String {
    format!("{BINDING_BASE}/{SLOT}/")
}

fn slot_schema() -> String {
    format!("{CUSTOM_SCHEMA_PREFIX}:{}", slot_path())
}

fn current_bindings() -> Result<Vec<String>, String> {
    let output = Command::new("gsettings")
        .args(["get", MEDIA_KEYS_SCHEMA, "custom-keybindings"])
        .output()
        .map_err(|e| format!("cannot run gsettings: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "gsettings failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(parse_binding_list(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse the GVariant array-of-strings form, e.g. `['/a/', '/b/']`.
///
/// An unset or empty key prints as `@as []`.
fn parse_binding_list(raw: &str) -> Vec<String> {
    let inner = raw
        .trim()
        .trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');

    inner
        .split(',')
        .map(|item| item.trim().trim_matches('\'').trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Render a list of paths back into the GVariant form `gsettings` expects.
fn render_binding_list(paths: &[String]) -> String {
    if paths.is_empty() {
        return "@as []".to_string();
    }
    let quoted: Vec<String> = paths.iter().map(|p| format!("'{p}'")).collect();
    format!("[{}]", quoted.join(", "))
}

fn run_gsettings(args: &[&str]) -> Result<(), String> {
    let output = Command::new("gsettings")
        .args(args)
        .output()
        .map_err(|e| format!("cannot run gsettings: {e}"))?;

    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "gsettings {} failed: {}",
        args.first().copied().unwrap_or(""),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Bind `binding` (default `Ctrl+Alt+V`) to `exe_path --toggle`.
///
/// Existing custom shortcuts are preserved; the slot is only added when absent
/// and rewritten in place when it already exists.
pub fn install(exe_path: &str, binding: Option<&str>, default_binding: &str) -> Result<String, String> {
    let binding = binding.unwrap_or(default_binding);
    let slot = slot_path();

    let mut bindings = current_bindings()?;
    if !bindings.iter().any(|p| p == &slot) {
        bindings.push(slot.clone());
        run_gsettings(&[
            "set",
            MEDIA_KEYS_SCHEMA,
            "custom-keybindings",
            &render_binding_list(&bindings),
        ])?;
    }

    let schema = slot_schema();
    let command = format!("{exe_path} --toggle");
    run_gsettings(&["set", &schema, "name", SHORTCUT_NAME])?;
    run_gsettings(&["set", &schema, "command", &command])?;
    run_gsettings(&["set", &schema, "binding", binding])?;

    Ok(binding.to_string())
}

/// Remove the shortcut slot, leaving every other custom shortcut untouched.
pub fn uninstall() -> Result<(), String> {
    let slot = slot_path();
    let mut bindings = current_bindings()?;
    let before = bindings.len();
    bindings.retain(|p| p != &slot);

    if bindings.len() != before {
        run_gsettings(&[
            "set",
            MEDIA_KEYS_SCHEMA,
            "custom-keybindings",
            &render_binding_list(&bindings),
        ])?;
    }

    // Clearing the keys avoids a stale command lingering in the schema.
    let schema = slot_schema();
    let _ = run_gsettings(&["reset", &schema, "binding"]);
    let _ = run_gsettings(&["reset", &schema, "command"]);
    let _ = run_gsettings(&["reset", &schema, "name"]);
    Ok(())
}

/// Report whether our slot is currently registered, for the settings UI.
pub fn status() -> Result<bool, String> {
    Ok(current_bindings()?.iter().any(|p| p == &slot_path()))
}

/// Absolute path of the running executable, used as the shortcut command.
pub fn current_exe() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("cannot resolve executable path: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_and_populated_lists() {
        assert!(parse_binding_list("@as []").is_empty());
        assert!(parse_binding_list("[]").is_empty());
        assert_eq!(
            parse_binding_list("['/a/', '/b/']"),
            vec!["/a/".to_string(), "/b/".to_string()]
        );
    }

    #[test]
    fn render_round_trips() {
        assert_eq!(render_binding_list(&[]), "@as []");
        let list = vec!["/a/".to_string(), "/b/".to_string()];
        assert_eq!(render_binding_list(&list), "['/a/', '/b/']");
        assert_eq!(parse_binding_list(&render_binding_list(&list)), list);
    }

    #[test]
    fn slot_path_is_slash_terminated() {
        assert!(slot_path().ends_with('/'));
        assert!(slot_schema().contains(CUSTOM_SCHEMA_PREFIX));
    }
}
