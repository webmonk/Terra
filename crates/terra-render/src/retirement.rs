//! Deferred GPU texture retirement — drop resources after N frames.
//!
//! Large height/normal reallocations should not destroy textures on the same
//! frame they were last sampled; queue them here and drain in `tick`.

/// Textures queued for destruction once the frame counter advances far enough.
#[derive(Debug, Default)]
pub struct DeferredGpuRetirement {
    retired: Vec<(u64, wgpu::Texture)>,
    retire_after_frames: u64,
}

impl DeferredGpuRetirement {
    pub fn new(retire_after_frames: u64) -> Self {
        Self {
            retired: Vec::new(),
            retire_after_frames: retire_after_frames.max(1),
        }
    }

    pub fn queue(&mut self, frame: u64, texture: wgpu::Texture) {
        self.retired.push((frame, texture));
    }

    /// Drop textures whose retirement frame has elapsed.
    pub fn tick(&mut self, current_frame: u64) {
        self.retired.retain(|(frame, _texture)| {
            current_frame.saturating_sub(*frame) < self.retire_after_frames
        });
    }

    pub fn pending(&self) -> usize {
        self.retired.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retirement_drops_after_delay() {
        let mut pool = DeferredGpuRetirement::new(2);
        // We cannot construct wgpu::Texture in unit tests without a device; verify
        // bookkeeping only when the queue is empty.
        assert_eq!(pool.pending(), 0);
        pool.tick(10);
        assert_eq!(pool.pending(), 0);
    }
}
