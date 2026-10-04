// ============================================================
// Sift - Organizing Guard
// Closing the window or quitting while photos are being archived, exported
// or deleted would cut the work off between two files. The frontend reports
// when it is organizing; meanwhile close and quit requests are refused and
// the frontend is told, so it can say why.
// ============================================================

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

/// Event sent when a close or quit request was refused
const CLOSE_BLOCKED_EVENT: &str = "close-blocked";

/// Whether the frontend is organizing photos right now
#[derive(Default)]
pub struct Organizing(AtomicBool);

impl Organizing {
    fn set(&self, active: bool) {
        self.0.store(active, Ordering::SeqCst);
    }

    fn is_active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[tauri::command]
pub fn set_organizing(state: State<Organizing>, active: bool) {
    state.set(active);
}

/// True if a close or quit must be refused right now; tells the frontend.
pub fn refuse_while_organizing(app: &AppHandle) -> bool {
    let is_organizing = app
        .try_state::<Organizing>()
        .is_some_and(|state| state.is_active());
    if is_organizing {
        let _ = app.emit(CLOSE_BLOCKED_EVENT, ());
    }
    is_organizing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_inactive_until_the_frontend_says_otherwise() {
        let organizing = Organizing::default();
        assert!(!organizing.is_active());

        organizing.set(true);
        assert!(organizing.is_active());

        organizing.set(false);
        assert!(!organizing.is_active());
    }
}
