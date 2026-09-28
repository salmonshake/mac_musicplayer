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
pub struct Queue {
    items: Vec<PathBuf>,
    order: Vec<usize>,
    pos: usize,
    shuffle: bool,
    #[expect(dead_code)]
    repeat: RepeatMode,
    rng: u64,
}

impl Queue {
    /// xorshift64 — simple fast PRNG seeded by `seed.max(1)` in `new`.
    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// Build a playback order with `first` at index 0, rest shuffled.
    fn shuffled_order(&mut self, first: usize) -> Vec<usize> {
        let mut rest: Vec<usize> = (0..self.items.len()).filter(|&i| i != first).collect();
        for i in (1..rest.len()).rev() {
            let j = (self.next_rand() % (i as u64 + 1)) as usize;
            rest.swap(i, j);
        }
        let mut v = vec![first];
        v.extend(rest);
        v
    }

    /// Create a new queue.
    ///
    /// `start` is an index into `items` (clamped to 0 if out of range).
    /// When `shuffle` is true the current track (`items[start]`) is placed
    /// first and the remaining tracks are shuffled with a deterministic seed.
    pub fn new(items: Vec<PathBuf>, start: usize, shuffle: bool, repeat: RepeatMode, seed: u64) -> Self {
        let order: Vec<usize> = (0..items.len()).collect();
        let pos = if start < items.len() { start } else { 0 };
        let mut q = Self {
            items,
            order,
            pos,
            shuffle,
            repeat,
            rng: seed.max(1),
        };
        if shuffle {
            q.shuffle = false; // prevent recursion
            q.set_shuffle(true);
        }
        q
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

    /// Enable or disable shuffle.
    ///
    /// When enabling: the current track is moved to the front and the rest
    /// are shuffled deterministically.  When disabling: the original order
    /// (0..n) is restored and `pos` is set to the index of the current
    /// track within that order.
    pub fn set_shuffle(&mut self, on: bool) {
        if self.items.is_empty() {
            self.shuffle = on;
            return;
        }
        let cur = self.order[self.pos];
        if on {
            self.order = self.shuffled_order(cur);
            self.pos = 0;
        } else {
            self.order = (0..self.items.len()).collect();
            self.pos = cur;
        }
        self.shuffle = on;
    }

    /// Return whether shuffle is currently enabled.
    pub fn shuffle(&self) -> bool {
        self.shuffle
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

    /// Helper: create a shuffled queue with n tracks, starting at `start`.
    fn qs(n: usize, start: usize) -> Queue {
        let items: Vec<PathBuf> = (0..n).map(|i| PathBuf::from(format!("{i}.flac"))).collect();
        Queue::new(items, start, true, RepeatMode::Off, 42)
    }

    #[test]
    fn test_shuffle_contains_all_tracks() {
        // 10 tracks, start=3: current is 3, next 9 times → all 0..10 exactly once
        let mut q = qs(10, 3);
        assert_eq!(q.current().map(|(i, _)| i), Some(3));
        let mut collected = vec![3u64]; // current track
        for _ in 0..9 {
            let idx = q.next().map(|(i, _)| i as u64);
            collected.push(idx.unwrap_or(u64::MAX)); // None at end
        }
        // Remove the sentinel (u64::MAX from end) and sort
        collected.retain(|&x| x != u64::MAX);
        collected.sort();
        assert_eq!(collected, (0..10u64).collect::<Vec<_>>());
    }

    #[test]
    fn test_shuffle_deterministic_same_seed() {
        // Same seed 42 → identical order from two independent queues
        let mut a = qs(10, 3);
        let mut b = qs(10, 3);
        let mut order_a = Vec::new();
        let mut order_b = Vec::new();
        // Collect all next() results until None
        while let Some((i, _)) = a.next() {
            order_a.push(i);
        }
        while let Some((i, _)) = b.next() {
            order_b.push(i);
        }
        assert_eq!(order_a, order_b);
    }

    #[test]
    fn test_set_shuffle_false_restores_order() {
        let mut q = qs(10, 3);
        // Advance 4 times: current goes 3 → order[1] → order[2] → order[3]
        q.next();
        q.next();
        q.next();
        let cur_item = q.current().map(|(i, _)| i).unwrap();
        // Disable shuffle: order becomes 0..10, pos = cur_item
        q.set_shuffle(false);
        assert!(!q.shuffle());
        assert_eq!(q.current().map(|(i, _)| i), Some(cur_item));
        // Next should go to cur_item + 1 (or None if cur_item is 9)
        let next_result = q.next().map(|(i, _)| i);
        if cur_item < 9 {
            assert_eq!(next_result, Some(cur_item + 1));
        } else {
            assert!(next_result.is_none());
        }
    }

    #[test]
    fn test_set_shuffle_empty_queue() {
        let mut q = q(0, 0);
        // Should not panic
        q.set_shuffle(true);
        assert!(q.current().is_none());
        assert!(q.next().is_none());
    }
}
