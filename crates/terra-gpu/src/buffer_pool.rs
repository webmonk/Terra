//! Reusable GPU buffer pool to reduce allocations across sim passes.

use std::collections::HashMap;

pub struct BufferPool {
    device: wgpu::Device,
    free: HashMap<u64, Vec<wgpu::Buffer>>,
}

impl BufferPool {
    pub fn new(device: wgpu::Device) -> Self {
        Self {
            device,
            free: HashMap::new(),
        }
    }

    pub fn acquire(&mut self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        if let Some(list) = self.free.get_mut(&size) {
            if let Some(buf) = list.pop() {
                return buf;
            }
        }
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pool-buf"),
            size,
            usage: usage
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn release(&mut self, size: u64, buf: wgpu::Buffer) {
        self.free.entry(size).or_default().push(buf);
    }

    pub fn clear(&mut self) {
        self.free.clear();
    }
}
