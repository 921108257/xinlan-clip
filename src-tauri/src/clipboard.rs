//! System clipboard access, deduplicated against our own writes.
//!
//! # Wayland behaviour
//!
//! GNOME 46 exposes no clipboard-change signal to background clients (the
//! portal's `SelectionOwnerChanged` exists in the interface but is never
//! emitted on this build — verified by listening for 20s across a clipboard
//! write). Capturing therefore happens on demand: whenever the panel is shown
//! or focused, and once a second while it stays open.
//!
//! Because pasting writes to the clipboard too, every capture would otherwise
//! re-record our own echo. [`write_text`] records the hash so [`read_text`]
//! callers can skip it.

use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

/// Hash of the last string this application put on the clipboard, or 0.
static SELF_WRITTEN_HASH: AtomicU64 = AtomicU64::new(0);

/// Read the clipboard as text.
///
/// Returns `Ok(None)` when the clipboard holds no text (an image, or nothing at
/// all) and `Err` when the platform refuses access.
pub fn read_text() -> Result<Option<String>, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Put `text` on the clipboard and remember it as our own write.
pub fn write_text(text: &str) -> Result<(), String> {
    SELF_WRITTEN_HASH.store(hash_u64(text), Ordering::SeqCst);
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text.to_owned()).map_err(|e| e.to_string())
}

/// True when `hash` is the fingerprint of our own most recent clipboard write.
pub fn is_self_written(hash: u64) -> bool {
    hash != 0 && SELF_WRITTEN_HASH.load(Ordering::SeqCst) == hash
}

/// Stable fingerprint of clipboard text, used for deduplication.
///
/// The full SHA-256 goes into SQLite as `content_hash`; the folded 64-bit form
/// is how the in-process self-write check compares values cheaply.
pub fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Fold a string into a 64-bit value for the self-write guard.
pub fn hash_u64(text: &str) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_distinct() {
        assert_eq!(hash_text("hello"), hash_text("hello"));
        assert_ne!(hash_text("hello"), hash_text("world"));
        assert_eq!(hash_text("").len(), 64);
    }

    #[test]
    fn self_written_guard_matches_only_last_write() {
        let first = hash_u64("first");
        let second = hash_u64("second");
        SELF_WRITTEN_HASH.store(first, Ordering::SeqCst);
        assert!(is_self_written(first));
        assert!(!is_self_written(second));
        assert!(!is_self_written(0));
    }
}
