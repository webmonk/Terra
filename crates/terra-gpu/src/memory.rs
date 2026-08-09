//! GPU/CPU memory budget controller.

/// Tracks allocated / pooled GPU-side bytes against a soft ceiling.
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    pub max_bytes: u64,
    pub used_bytes: u64,
}

impl MemoryBudget {
    pub fn new(max_mb: u64) -> Self {
        Self {
            max_bytes: max_mb * 1024 * 1024,
            used_bytes: 0,
        }
    }

    pub fn try_alloc(&mut self, bytes: u64) -> bool {
        if self.used_bytes + bytes > self.max_bytes {
            return false;
        }
        self.used_bytes += bytes;
        true
    }

    pub fn free(&mut self, bytes: u64) {
        self.used_bytes = self.used_bytes.saturating_sub(bytes);
    }

    pub fn utilization(&self) -> f32 {
        self.used_bytes as f32 / self.max_bytes.max(1) as f32
    }

    /// Remaining headroom in bytes.
    pub fn remaining(&self) -> u64 {
        self.max_bytes.saturating_sub(self.used_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_rejects_over() {
        let mut b = MemoryBudget::new(1);
        assert!(b.try_alloc(512 * 1024));
        assert!(!b.try_alloc(600 * 1024));
    }
}
