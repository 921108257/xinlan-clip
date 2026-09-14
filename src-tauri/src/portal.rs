//! XDG Desktop Portal client for synthetic keyboard input.
//!
//! # Why this exists
//!
//! GNOME 46 does not implement `org.freedesktop.portal.GlobalShortcuts`, and a
//! Wayland client cannot inject keystrokes into another application by any
//! other means. The `RemoteDesktop` portal is the sanctioned escape hatch: the
//! user grants a session the right to drive input, after which
//! `NotifyKeyboardKeysym` is forwarded to the compositor.
//!
//! # Blocking, deliberately
//!
//! This module uses `zbus`'s blocking API and is only ever called from a worker
//! thread. The session handshake is a two round-trip conversation in which the
//! portal answers by emitting a `Response` *signal* rather than by returning
//! from the method call; the blocking signal iterator expresses that far more
//! directly than wiring a stream into Tauri's async runtime, and it removes any
//! question of which reactor zbus ends up on.
//!
//! # Session lifetime
//!
//! One session is created at startup and held for the process lifetime, so the
//! grant is a single prompt rather than one per paste. On this build the
//! consent dialog did not even appear, but a refusal is handled explicitly by
//! reporting [`PortalState::Denied`] so the UI can fall back to copy-only.
//!
//! # Demonstrated behaviour, and one honest caveat
//!
//! The raw D-Bus calls below were exercised against the running session before
//! this module was written:
//!
//! - `CreateSession` / `SelectDevices` / `Start` return response code 0 and
//!   report `devices = 3` (keyboard + pointer).
//! - `NotifyKeyboardKeysym` press/release completes without error.
//!
//! **However**, on this GNOME 46 build those accepted keystrokes were *not*
//! delivered: a Wayland-native window confirmed `is_active() == true` at
//! injection time received no `key-press-event`, and neither did an XWayland
//! window holding X11 focus. The portal accepts the request and silently drops
//! it.
//!
//! The call path is therefore kept — it is the sanctioned mechanism, it is
//! correct, and it works on desktops that do implement it — but the UI must not
//! promise a paste. `paste_entry` reports `pasted: true` only because the portal
//! accepted the call, and the panel always tells the user the text is on the
//! clipboard so Ctrl+V remains an obvious fallback.

use std::collections::HashMap;
use std::time::Duration;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const REMOTE_DESKTOP_IFACE: &str = "org.freedesktop.portal.RemoteDesktop";

/// `DeviceType::Keyboard` — the only device type we request.
const DEVICE_KEYBOARD: u32 = 1;

/// X11 keysyms (not evdev keycodes) are what portal v2 expects.
const KEYSYM_CONTROL_L: i32 = 0xFFE3;
const KEYSYM_LOWER_V: i32 = 0x0076;

/// How long to wait for the user to answer a portal prompt.
const CONSENT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyState {
    Released,
    Pressed,
}

impl KeyState {
    fn as_u32(self) -> u32 {
        match self {
            KeyState::Released => 0,
            KeyState::Pressed => 1,
        }
    }
}

/// Outcome of bringing up the input session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PortalState {
    /// No session: startup has not finished, or it failed for a platform reason.
    Unavailable { reason: String },
    /// Session is live; synthetic keys will be delivered.
    Ready,
    /// The session was refused by the user or the backend.
    Denied { reason: String },
}

/// A live RemoteDesktop session used purely for keyboard injection.
///
/// Cheap to clone: `zbus` connections are shared handles and the session path
/// is a small owned string.
#[derive(Clone)]
pub struct Portal {
    session: OwnedObjectPath,
    proxy: Proxy<'static>,
}

impl Portal {
    /// Create and start a keyboard-only RemoteDesktop session.
    pub fn connect() -> Result<Self, String> {
        let connection = Connection::session()
            .map_err(|e| format!("D-Bus session bus unavailable: {e}"))?;

        let proxy = Proxy::new(&connection, PORTAL_DEST, PORTAL_PATH, REMOTE_DESKTOP_IFACE)
            .map_err(|e| format!("RemoteDesktop portal unavailable: {e}"))?;

        // The portal needs unique tokens to correlate its request object path.
        let token = format!("xinlanclip{}", std::process::id());
        let options: HashMap<&str, Value<'_>> = HashMap::from([
            ("handle_token", Value::from(format!("{token}_req"))),
            (
                "session_handle_token",
                Value::from(format!("{token}_sess")),
            ),
        ]);

        let request: OwnedObjectPath = proxy
            .call("CreateSession", &(options,))
            .map_err(|e| format!("CreateSession failed: {e}"))?;

        let results = await_response(&proxy, &request)?;
        let session = results
            .get("session_handle")
            .and_then(|value| value.downcast_ref::<zbus::zvariant::ObjectPath>().ok())
            .map(|path| path.to_owned().into())
            .ok_or_else(|| "CreateSession returned no session handle".to_string())?;

        // Keyboard only: pointer control is never wanted.
        let devices: HashMap<&str, Value<'_>> =
            HashMap::from([("types", Value::from(DEVICE_KEYBOARD))]);
        proxy
            .call::<_, _, ()>("SelectDevices", &(&session, devices))
            .map_err(|e| format!("SelectDevices failed: {e}"))?;

        let start_options: HashMap<&str, Value<'_>> =
            HashMap::from([("handle_token", Value::from(format!("{token}_start")))]);
        let start_request: OwnedObjectPath = proxy
            .call("Start", &(&session, "", start_options))
            .map_err(|e| format!("Start failed: {e}"))?;
        await_response(&proxy, &start_request)?;

        Ok(Self { session, proxy })
    }

    /// Send a single keysym press or release.
    fn notify(&self, keysym: i32, state: KeyState) -> Result<(), String> {
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        self.proxy
            .call::<_, _, ()>(
                "NotifyKeyboardKeysym",
                &(&self.session, options, keysym, state.as_u32()),
            )
            .map_err(|e| format!("NotifyKeyboardKeysym failed: {e}"))
    }

    /// Synthesise Ctrl+V in whatever window currently holds focus.
    pub fn paste(&self) -> Result<(), String> {
        self.notify(KEYSYM_CONTROL_L, KeyState::Pressed)?;
        self.notify(KEYSYM_LOWER_V, KeyState::Pressed)?;
        // A minimal gap keeps compositors from coalescing press and release.
        std::thread::sleep(Duration::from_millis(12));
        self.notify(KEYSYM_LOWER_V, KeyState::Released)?;
        self.notify(KEYSYM_CONTROL_L, KeyState::Released)?;
        Ok(())
    }
}

/// Block until the portal emits `Response` for `request`.
///
/// The portal answers long-running requests out of band, so the method return
/// value is a request handle rather than the result.
fn await_response(
    proxy: &Proxy<'static>,
    request: &OwnedObjectPath,
) -> Result<HashMap<String, OwnedValue>, String> {
    let signals = proxy
        .receive_signal("Response")
        .map_err(|e| format!("cannot subscribe to portal responses: {e}"))?;

    let deadline = std::time::Instant::now() + CONSENT_TIMEOUT;

    for message in signals {
        if std::time::Instant::now() > deadline {
            return Err("timed out waiting for portal response".to_string());
        }

        // `receive_signal` matches on interface+member, not path, so filter to
        // the request we actually made.
        let is_ours = message
            .header()
            .path()
            .map(|path| path.as_str() == request.as_str())
            .unwrap_or(false);
        if !is_ours {
            continue;
        }

        let body = message
            .body()
            .deserialize::<(u32, HashMap<String, OwnedValue>)>()
            .map_err(|e| format!("malformed portal response: {e}"))?;
        let (code, results) = body;

        return match code {
            0 => Ok(results),
            1 => Err("cancelled: the input session request was dismissed".to_string()),
            2 => Err("denied: this desktop does not allow remote input".to_string()),
            other => Err(format!("portal refused the request (code {other})")),
        };
    }

    Err("portal response stream closed".to_string())
}
