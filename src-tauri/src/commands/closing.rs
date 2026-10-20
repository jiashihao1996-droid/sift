// ============================================================
// Sift - Save Before Closing
// The frontend saves culling progress a moment after changes settle, so a
// close or quit right after a mark could lose it. The first close or quit
// request is held: the frontend is asked to save now and reports back, and
// then the window closes or the app quits. If it does not answer in time
// the close goes ahead anyway, so quitting never hangs.
// ============================================================

use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Event asking the frontend to save progress now
const FLUSH_EVENT: &str = "flush-before-close";
/// Saving a 20,000-photo folder takes tens of milliseconds
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// What to do once progress is saved
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    CloseWindow,
    Exit,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum GateState {
    #[default]
    Open,
    Waiting(After),
    Saved,
}

/// Holds the first close or quit until the frontend has saved
#[derive(Default)]
pub struct SaveGate(Mutex<GateState>);

impl SaveGate {
    /// Whether the request must wait; `Some(true)` when it starts the wait
    fn hold(&self, after: After) -> Option<bool> {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match *state {
            GateState::Saved => None,
            GateState::Open => {
                *state = GateState::Waiting(after);
                Some(true)
            }
            // Quitting while a close waits: quit once saved
            GateState::Waiting(_) => {
                if after == After::Exit {
                    *state = GateState::Waiting(After::Exit);
                }
                Some(false)
            }
        }
    }

    /// End the wait; what to do now, unless nothing was waiting
    fn release(&self) -> Option<After> {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match *state {
            GateState::Waiting(after) => {
                *state = GateState::Saved;
                Some(after)
            }
            _ => None,
        }
    }
}

/// True if this close or quit must wait for progress to be saved; asks
/// the frontend to save and starts the timeout the first time.
pub fn hold_for_save(app: &AppHandle, after: After) -> bool {
    let Some(gate) = app.try_state::<SaveGate>() else {
        return false;
    };
    match gate.hold(after) {
        None => false,
        Some(false) => true,
        Some(true) => {
            if let Err(e) = app.emit(FLUSH_EVENT, ()) {
                eprintln!("[Sift] Failed to ask the frontend to save: {}", e);
            }
            let app = app.clone();
            thread::spawn(move || {
                thread::sleep(FLUSH_TIMEOUT);
                proceed(&app);
            });
            true
        }
    }
}

/// The frontend has saved its progress
#[tauri::command]
pub fn progress_saved(app: AppHandle) {
    proceed(&app);
}

fn proceed(app: &AppHandle) {
    let Some(after) = app.try_state::<SaveGate>().and_then(|gate| gate.release()) else {
        return;
    };
    match after {
        After::CloseWindow => match app.get_webview_window("main") {
            Some(window) => {
                if let Err(e) = window.close() {
                    eprintln!("[Sift] Failed to close the window: {}", e);
                }
            }
            None => app.exit(0),
        },
        After::Exit => app.exit(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_the_first_request_until_released() {
        let gate = SaveGate::default();

        assert_eq!(gate.hold(After::CloseWindow), Some(true));
        assert_eq!(gate.release(), Some(After::CloseWindow));
    }

    #[test]
    fn lets_everything_through_once_saved() {
        let gate = SaveGate::default();
        gate.hold(After::CloseWindow);
        gate.release();

        assert_eq!(gate.hold(After::CloseWindow), None);
        assert_eq!(gate.hold(After::Exit), None);
    }

    #[test]
    fn a_second_request_while_waiting_waits_without_asking_again() {
        let gate = SaveGate::default();
        gate.hold(After::CloseWindow);

        assert_eq!(gate.hold(After::CloseWindow), Some(false));
    }

    #[test]
    fn quitting_while_a_close_waits_quits_once_saved() {
        let gate = SaveGate::default();
        gate.hold(After::CloseWindow);
        gate.hold(After::Exit);

        assert_eq!(gate.release(), Some(After::Exit));
    }

    #[test]
    fn a_late_release_does_nothing() {
        // The frontend answers after the timeout already let the close through
        let gate = SaveGate::default();
        gate.hold(After::Exit);
        gate.release();

        assert_eq!(gate.release(), None);
    }

    #[test]
    fn a_release_with_nothing_waiting_does_nothing() {
        assert_eq!(SaveGate::default().release(), None);
    }
}
