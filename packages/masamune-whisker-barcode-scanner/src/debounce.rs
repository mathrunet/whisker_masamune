use std::time::{Duration, Instant};

/// Drops repeated reports of the same code.
///
/// A code is accepted when it differs from the last one seen, or when the last sighting of the
/// same code is at least the window ago. Every sighting (accepted or not) restarts the window, so
/// a code held in front of the camera is reported once and again only after it has been out of
/// view for the whole window.
#[derive(Debug, Clone)]
pub struct ScanDebouncer {
    window_ms: u64,
    last: Option<(String, u64)>,
    origin: Instant,
}

impl ScanDebouncer {
    /// A debouncer with the given window (zero accepts every report).
    pub fn new(window: Duration) -> Self {
        Self {
            window_ms: window.as_millis().min(u64::MAX as u128) as u64,
            last: None,
            origin: Instant::now(),
        }
    }

    /// Whether `value`, seen now, should be reported.
    pub fn accept(&mut self, value: &str) -> bool {
        let now = self.origin.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.accept_at(value, now)
    }

    /// Whether `value`, seen at `now_ms` (any monotonic millisecond clock), should be reported.
    pub fn accept_at(&mut self, value: &str, now_ms: u64) -> bool {
        let accepted = match &self.last {
            Some((last, at)) if last == value => now_ms.saturating_sub(*at) >= self.window_ms,
            _ => true,
        };
        match &mut self.last {
            Some((last, at)) if last == value => *at = now_ms,
            _ => self.last = Some((value.to_owned(), now_ms)),
        }
        accepted
    }

    /// Forgets the last code, so the next report is accepted.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_the_same_code_while_it_stays_in_view() {
        let mut debouncer = ScanDebouncer::new(Duration::from_millis(1000));
        assert!(debouncer.accept_at("a", 0));
        assert!(!debouncer.accept_at("a", 400));
        // Each sighting restarts the window.
        assert!(!debouncer.accept_at("a", 1300));
        assert!(debouncer.accept_at("a", 2300));
        // A different code is reported at once, and so is the first one after it.
        assert!(debouncer.accept_at("b", 2400));
        assert!(debouncer.accept_at("a", 2500));
        debouncer.reset();
        assert!(debouncer.accept_at("a", 2600));
    }

    #[test]
    fn zero_window_accepts_everything() {
        let mut debouncer = ScanDebouncer::new(Duration::ZERO);
        assert!(debouncer.accept_at("a", 0));
        assert!(debouncer.accept_at("a", 0));
    }
}
