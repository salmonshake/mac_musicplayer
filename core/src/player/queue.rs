use std::path::{Path, PathBuf};

/// Repeat mode for queue playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// Playback queue.
///
/// `items` holds tracks in original order.
/// `order` is the playback sequence (indices into `items`).
/// `pos` is the current position within `order`.
#[allow(dead_code)]
// 7.3b/7.3c: shuffle, repeat, and rng will be used in those tasks.
pub struct Queue {
    items: Vec<PathBuf>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    repeat: RepeatMode,
    rng: u64,
}

impl Queue {
    /// Create a new queue.
    ///
    /// `start` is an index into `items` (clamped to 0 if out of range).
    /// Shuffle and repeat are stored but not yet applied (order is 0..n).
    pub fn new(items: Vec<PathBuf>, start: usize, shuffle: bool, repeat: RepeatMode, seed: u64) -> Self {
        let order: Vec<usize> = (0..items.len()).collect();
        let pos = if start < items.len() { start } else { 0 };
        Self {
            items,
            order,
            pos,
            shuffle,
            repeat,
            rng: seed.max(1),
        }
    }

    /// Return the current track (index into `items` and path). Returns None if empty.
    pub fn current(&self) -> Option<(usize, &Path)> {
        let i = *self.order.get(self.pos)?;
        Some((i, self.items.get(i)?.as_path()))
    }

    /// Advance to the next track.
    /// Returns None at the end (position unchanged).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<(usize, &Path)> {
        if self.pos + 1 >= self.order.len() {
            return None;
        }
        self.pos += 1;
        self.current()
    }

    /// Go to the previous track.
    /// At the beginning, stays at the first track and returns current.
    pub fn prev(&mut self) -> Option<(usize, &Path)> {
        if self.pos > 0 {
            self.pos -= 1;
        }
        self.current()
    }

    /// Return all items in original order.
    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Queue, RepeatMode};

    /// Helper: create a queue with n tracks ("0.flac".."n-1.flac"), starting at `start`.
    fn q(n: usize, start: usize) -> Queue {
        let items: Vec<PathBuf> = (0..n).map(|i| PathBuf::from(format!("{i}.flac"))).collect();
        Queue::new(items, start, false, RepeatMode::default(), 42)
    }

    #[test]
    fn test_forward_playback() {
        let mut q = q(3, 0);
        // Start at track 0
        assert_eq!(q.current().map(|(i, _)| i), Some(0));

        // Advance: 0 -> 1 -> 2
        assert_eq!(q.next().map(|(i, _)| i), Some(1));
        assert_eq!(q.next().map(|(i, _)| i), Some(2));

        // Next at end returns None, current stays at 2
        assert!(q.next().is_none());
        assert_eq!(q.current().map(|(i, _)| i), Some(2));
    }

    #[test]
    fn test_prev_stays_at_beginning() {
        let mut q = q(3, 1);
        // Start at track 1
        assert_eq!(q.current().map(|(i, _)| i), Some(1));

        // Prev: 1 -> 0
        assert_eq!(q.prev().map(|(i, _)| i), Some(0));

        // Prev at 0 stays at 0
        assert_eq!(q.prev().map(|(i, _)| i), Some(0));
    }

    #[test]
    fn test_empty_queue() {
        let mut q = q(0, 0);
        assert!(q.current().is_none());
        assert!(q.next().is_none());
        assert!(q.prev().is_none());
    }

    #[test]
    fn test_out_of_range_start_clamped_to_zero() {
        let q = q(3, 9);
        // start=9 is out of range for 3 items, should clamp to 0
        assert_eq!(q.current().map(|(i, _)| i), Some(0));
    }
}
