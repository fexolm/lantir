use crate::resource::{DeferDrop, Resource};
use crate::{RenderEngine, UpdateFrequency};
use ash::vk;
use std::sync::{Arc, Mutex};
use vk_mem::{Alloc, Allocation};

pub type Buffer = Resource<BufferData>;

impl Buffer {
    pub fn new(engine: Arc<RenderEngine>, create_info: &BufferCreateInfo) -> anyhow::Result<Self> {
        let data = BufferData::new(&engine, create_info)?;
        Ok(Resource::make(engine, data))
    }

    pub fn get_buffer(&self) -> vk::Buffer {
        self.get_handle()
            .get_buffer(self.engine.get_current_frame_index())
    }

    fn get_allocation(&self) -> &Mutex<Allocation> {
        self.get_handle()
            .get_allocation(self.engine.get_current_frame_index())
    }

    pub fn get_device_address(&self) -> u64 {
        let buffer = self.get_buffer();
        let address_info = vk::BufferDeviceAddressInfo::default().buffer(buffer);
        unsafe { self.engine.device.get_buffer_device_address(&address_info) }
    }

    pub unsafe fn map(&self) -> anyhow::Result<*mut u8> {
        let mut allocation = self.get_allocation().lock().unwrap();
        Ok(self.engine.allocator.map_memory(&mut allocation)?)
    }

    pub unsafe fn unmap(&self) {
        let mut allocation = self.get_allocation().lock().unwrap();
        self.engine.allocator.unmap_memory(&mut allocation);
    }
}
pub struct BufferData {
    buffers: Vec<vk::Buffer>,
    allocations: Vec<Mutex<Allocation>>,
}

pub struct BufferCreateInfo {
    pub size: vk::DeviceSize,
    pub usage: vk::BufferUsageFlags,
    pub update_frequency: UpdateFrequency,
    pub memory_property: vk::MemoryPropertyFlags,
    pub vma_flags: vk_mem::AllocationCreateFlags,
}

impl BufferData {
    pub fn new(engine: &RenderEngine, create_info: &BufferCreateInfo) -> anyhow::Result<Self> {
        let frame_count = match create_info.update_frequency {
            UpdateFrequency::Static => 1,
            UpdateFrequency::PerFrame => engine.frames.len(),
        };

        let mut buffers = Vec::with_capacity(frame_count);
        let mut allocations = Vec::with_capacity(frame_count);

        for _ in 0..frame_count {
            let buffer_info = vk::BufferCreateInfo::default()
                .size(create_info.size)
                .usage(create_info.usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let alloc_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                required_flags: create_info.memory_property,
                flags: create_info.vma_flags,
                ..Default::default()
            };

            let (buffer, allocation) =
                unsafe { engine.allocator.create_buffer(&buffer_info, &alloc_info)? };

            buffers.push(buffer);
            allocations.push(Mutex::new(allocation));
        }

        Ok(Self {
            buffers,
            allocations,
        })
    }

    pub fn get_buffer(&self, frame_index: usize) -> vk::Buffer {
        if self.buffers.len() == 1 {
            self.buffers[0]
        } else {
            self.buffers[frame_index]
        }
    }

    pub fn get_allocation(&self, frame_index: usize) -> &Mutex<Allocation> {
        if self.allocations.len() == 1 {
            &self.allocations[0]
        } else {
            &self.allocations[frame_index]
        }
    }
}

impl DeferDrop for BufferData {
    fn destroy(&mut self, _engine: &crate::RenderEngine) {
        for (&buffer, allocation) in self.buffers.iter().zip(self.allocations.iter()) {
            unsafe {
                _engine
                    .allocator
                    .destroy_buffer(buffer, &mut allocation.lock().unwrap());
            }
        }
    }
}
