use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// Copies secrets to the system clipboard and clears them again after a delay,
/// but only if the clipboard still holds what we put there.
pub struct ClipboardGuard {
    inner: Option<arboard::Clipboard>,
    pending: Option<Pending>,
}

struct Pending {
    digest: [u8; 32],
    clear_at: Option<Instant>,
}

impl ClipboardGuard {
    pub fn new() -> Self {
        Self { inner: arboard::Clipboard::new().ok(), pending: None }
    }

    pub fn copy(&mut self, text: &str, clear_after: Option<Duration>) -> Result<(), String> {
        let cb = self.inner.as_mut().ok_or("clipboard is unavailable")?;
        set_text(cb, text).map_err(|e| e.to_string())?;
        self.pending = Some(Pending { digest: digest(text), clear_at: clear_after.map(|d| Instant::now() + d) });
        Ok(())
    }

    /// Call every frame. Returns how long until the next scheduled clear.
    pub fn tick(&mut self) -> Option<Duration> {
        let at = self.pending.as_ref()?.clear_at?;
        let now = Instant::now();
        if now >= at {
            self.clear_if_ours();
            None
        } else {
            Some(at - now)
        }
    }

    /// Time left before the pending clear, for the status bar.
    pub fn remaining(&self) -> Option<Duration> {
        self.pending.as_ref()?.clear_at?.checked_duration_since(Instant::now())
    }

    pub fn clear_if_ours(&mut self) {
        let (Some(cb), Some(p)) = (self.inner.as_mut(), self.pending.take()) else { return };
        if cb.get_text().map(|t| digest(&t) == p.digest).unwrap_or(false) {
            let _ = cb.clear();
        }
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        self.clear_if_ours();
    }
}

#[cfg(windows)]
fn set_text(cb: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    // Keep secrets out of Win+V history and cloud clipboard sync.
    use arboard::SetExtWindows;
    cb.set().exclude_from_history().exclude_from_cloud().text(text)
}

#[cfg(not(windows))]
fn set_text(cb: &mut arboard::Clipboard, text: &str) -> Result<(), arboard::Error> {
    cb.set_text(text)
}

fn digest(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}
