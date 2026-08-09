//! Small reusable allocation pool for transient heightfield buffers.

/// Recycles dense float buffers between resampling and simulation passes.
///
/// Caps retained free buffers so amplify / multilevel schedules at 2K–4K do not
/// unbounded-grow the arena (Phase I memory budgeting).
#[derive(Debug)]
pub struct FloatArena {
    free: Vec<Vec<f32>>,
    /// Max retained empty buffers (capacity still varies per buffer).
    pub max_retained: usize,
    /// Approximate bytes retained in the free list (capacity × 4).
    retained_bytes: usize,
    /// Soft cap on retained free-list bytes (0 = unlimited bytes, still capped by count).
    pub max_retained_bytes: usize,
}

impl Default for FloatArena {
    fn default() -> Self {
        Self::new()
    }
}

impl FloatArena {
    pub fn new() -> Self {
        Self {
            free: Vec::new(),
            max_retained: 8,
            retained_bytes: 0,
            // ~64 MiB soft cap for recycled scratch (2K–4K aux-friendly).
            max_retained_bytes: 64 * 1024 * 1024,
        }
    }

    pub fn with_limits(max_retained: usize, max_retained_bytes: usize) -> Self {
        Self {
            free: Vec::new(),
            max_retained: max_retained.max(1),
            retained_bytes: 0,
            max_retained_bytes,
        }
    }

    pub fn acquire(&mut self, len: usize) -> Vec<f32> {
        if let Some(index) = self.free.iter().position(|buf| buf.capacity() >= len) {
            let mut buf = self.free.swap_remove(index);
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(buf.capacity() * std::mem::size_of::<f32>());
            buf.resize(len, 0.0);
            return buf;
        }
        vec![0.0; len]
    }

    pub fn release(&mut self, mut buf: Vec<f32>) {
        buf.clear();
        let bytes = buf.capacity() * std::mem::size_of::<f32>();
        if self.free.len() >= self.max_retained {
            // Drop the smallest retained buffer to make room if this one is larger.
            if let Some((i, _)) = self
                .free
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| b.capacity())
            {
                if self.free[i].capacity() < buf.capacity() {
                    let old = self.free.swap_remove(i);
                    self.retained_bytes = self
                        .retained_bytes
                        .saturating_sub(old.capacity() * std::mem::size_of::<f32>());
                } else {
                    return;
                }
            } else {
                return;
            }
        }
        if self.max_retained_bytes > 0 && self.retained_bytes + bytes > self.max_retained_bytes {
            return;
        }
        self.retained_bytes += bytes;
        self.free.push(buf);
    }

    pub fn clear(&mut self) {
        self.free.clear();
        self.retained_bytes = 0;
    }

    pub fn retained_count(&self) -> usize {
        self.free.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_caps_retained() {
        let mut a = FloatArena::with_limits(2, 0);
        let b0 = a.acquire(64);
        let b1 = a.acquire(64);
        let b2 = a.acquire(64);
        a.release(b0);
        a.release(b1);
        a.release(b2);
        assert!(a.retained_count() <= 2);
    }
}
