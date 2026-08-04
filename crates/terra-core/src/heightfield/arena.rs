//! Small reusable allocation pool for transient heightfield buffers.

/// Recycles dense float buffers between resampling and simulation passes.
#[derive(Debug, Default)]
pub struct FloatArena {
    free: Vec<Vec<f32>>,
}

impl FloatArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(&mut self, len: usize) -> Vec<f32> {
        if let Some(index) = self.free.iter().position(|buf| buf.capacity() >= len) {
            let mut buf = self.free.swap_remove(index);
            buf.resize(len, 0.0);
            return buf;
        }
        vec![0.0; len]
    }

    pub fn release(&mut self, mut buf: Vec<f32>) {
        buf.clear();
        self.free.push(buf);
    }
}
