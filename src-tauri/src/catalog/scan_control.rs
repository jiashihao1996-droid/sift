// ============================================================
// Sift - Scan Control
// Scanning a folder of RAW-only photos extracts each one's preview, which
// takes minutes when the RAWs must be read whole (CR3, RAF, NEF) from a slow
// drive. The frontend numbers each scan: starting another one, or cancelling,
// makes the running scan stale, and it stops before its next photo. A
// cancelled scan returns no photos at all (never part of the folder);
// previews it already extracted stay in the cache for the next scan.
// ============================================================

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Error of a scan that was cancelled or replaced by a newer one; the
/// frontend no longer waits for it, so it is never shown
pub const SCAN_CANCELLED: &str = "Scan cancelled";

/// No scan is running
const NO_SCAN: u32 = 0;

/// Progress is sent at most this often, plus when the last preview is done
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// The id of the scan that may keep running; every other scan is stale
#[derive(Default)]
pub struct ScanGate(AtomicU32);

impl ScanGate {
    /// Make `scan_id` the running scan; any earlier one becomes stale
    pub fn begin(&self, scan_id: u32) {
        self.0.store(scan_id, Ordering::SeqCst);
    }

    /// Stop `scan_id` if it is still the running scan. A newer scan that
    /// already began is left alone.
    pub fn cancel(&self, scan_id: u32) {
        let _ = self
            .0
            .compare_exchange(scan_id, NO_SCAN, Ordering::SeqCst, Ordering::SeqCst);
    }

    pub fn is_current(&self, scan_id: u32) -> bool {
        scan_id != NO_SCAN && self.0.load(Ordering::SeqCst) == scan_id
    }
}

/// What a scan asks of its caller while it runs
pub struct ScanControl<'a> {
    /// True once the scan should stop
    pub is_cancelled: &'a (dyn Fn() -> bool + Sync),
    /// `(done, total)` RAW-only previews extracted
    pub on_progress: &'a (dyn Fn(usize, usize) + Sync),
}

impl ScanControl<'_> {
    /// Runs to the end and reports nothing
    #[cfg(test)]
    pub const UNCONTROLLED: ScanControl<'static> = ScanControl {
        is_cancelled: &|| false,
        on_progress: &|_, _| {},
    };

    /// `Err(SCAN_CANCELLED)` once the scan should stop
    pub fn check(&self) -> Result<(), String> {
        if (self.is_cancelled)() {
            Err(SCAN_CANCELLED.to_string())
        } else {
            Ok(())
        }
    }
}

/// Counts extracted previews across worker threads and reports the count,
/// throttled so a fast folder doesn't flood the frontend with events
pub struct PreviewProgress<'a> {
    on_progress: &'a (dyn Fn(usize, usize) + Sync),
    total: usize,
    done: AtomicUsize,
    last_sent: Mutex<Instant>,
}

impl<'a> PreviewProgress<'a> {
    /// Reports `0 / total` right away, so the count shows before the first
    /// (possibly slow) preview is done. Nothing is reported when `total` is 0.
    pub fn start(control: &ScanControl<'a>, total: usize) -> Self {
        if total > 0 {
            (control.on_progress)(0, total);
        }
        PreviewProgress {
            on_progress: control.on_progress,
            total,
            done: AtomicUsize::new(0),
            last_sent: Mutex::new(Instant::now()),
        }
    }

    /// One more preview is done (extracted or found unusable)
    pub fn finish_one(&self) {
        let done = self.done.fetch_add(1, Ordering::SeqCst) + 1;
        let mut last_sent = match self.last_sent.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if done == self.total || last_sent.elapsed() >= PROGRESS_INTERVAL {
            *last_sent = Instant::now();
            (self.on_progress)(done, self.total);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_scan_makes_the_running_one_stale() {
        let gate = ScanGate::default();
        gate.begin(1);
        assert!(gate.is_current(1));

        gate.begin(2);
        assert!(!gate.is_current(1));
        assert!(gate.is_current(2));
    }

    #[test]
    fn cancelling_stops_only_that_scan() {
        let gate = ScanGate::default();
        gate.begin(1);
        gate.cancel(1);
        assert!(!gate.is_current(1));

        // A cancel that arrives after a newer scan began leaves it running
        gate.begin(2);
        gate.cancel(1);
        assert!(gate.is_current(2));
    }

    #[test]
    fn no_scan_is_current_before_one_begins() {
        let gate = ScanGate::default();
        assert!(!gate.is_current(0));
        assert!(!gate.is_current(1));
    }

    #[test]
    fn check_errs_once_cancelled() {
        assert!(ScanControl::UNCONTROLLED.check().is_ok());

        let control = ScanControl {
            is_cancelled: &|| true,
            on_progress: &|_, _| {},
        };
        assert_eq!(control.check(), Err(SCAN_CANCELLED.to_string()));
    }

    fn recorded(total: usize, finished: usize) -> Vec<(usize, usize)> {
        let events = Mutex::new(Vec::new());
        let on_progress = |done, total| events.lock().unwrap().push((done, total));
        let control = ScanControl {
            is_cancelled: &|| false,
            on_progress: &on_progress,
        };
        let progress = PreviewProgress::start(&control, total);
        for _ in 0..finished {
            progress.finish_one();
        }
        events.into_inner().unwrap()
    }

    #[test]
    fn reports_the_total_at_once_and_the_last_preview_always() {
        let events = recorded(500, 500);
        assert_eq!(events.first(), Some(&(0, 500)));
        assert_eq!(events.last(), Some(&(500, 500)));
        // Finishing 500 previews within a few milliseconds is throttled
        assert!(events.len() < 10, "{} events", events.len());
    }

    #[test]
    fn reports_nothing_without_previews_to_extract() {
        assert!(recorded(0, 0).is_empty());
    }
}
