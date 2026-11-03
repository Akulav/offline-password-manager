use std::sync::mpsc;
use std::thread;

use eframe::egui;

/// A piece of slow work (Argon2, file I/O) running off the UI thread.
pub struct Task<T> {
    rx: mpsc::Receiver<T>,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(ctx: &egui::Context, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        Self { rx }
    }

    pub fn poll(&self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// Take the finished result out of an optional task slot.
pub fn take<T: Send + 'static>(slot: &mut Option<Task<T>>) -> Option<T> {
    let result = slot.as_ref()?.poll()?;
    *slot = None;
    Some(result)
}
