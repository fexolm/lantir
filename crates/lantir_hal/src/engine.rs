use crate::CommandBuffer;
use crate::device::Device;
use crate::frame::RenderFrame;
use crate::instance::Instance;
use crate::resource::DeferDrop;
use crate::surface::Surface;
use crate::swapchain::{Swapchain, SwapchainImage};
use anyhow::anyhow;
use ash::vk;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use vk_mem::{Allocator, AllocatorCreateInfo};
use winit::window::Window;

pub struct RenderEngine {
    pub(crate) swapchain: Mutex<Swapchain>,
    pub(crate) frames: Vec<RenderFrame>,
    current_frame: Mutex<usize>,
    pub(crate) descriptor_pool: vk::DescriptorPool,

    pub(crate) allocator: Allocator,
    pub(crate) device: Device,
    pub(crate) surface: Surface,
    pub(crate) instance: Instance,

    immediate_command_buffer: Mutex<CommandBuffer>,

    is_started: AtomicBool,

    pub(crate) acceleration_structure_loader: ash::khr::acceleration_structure::Device,
    pub(crate) ray_tracing_pipeline_loader: ash::khr::ray_tracing_pipeline::Device,
}

pub struct RenderEngineConfig {
    pub debug: bool,
    pub frames_in_flight: usize,
}

impl RenderEngine {
    pub fn new(window: &Window, config: &RenderEngineConfig) -> anyhow::Result<Arc<Self>> {
        unsafe {
            let instance = Instance::new(window, config.debug)?;
            let surface = Surface::new(&instance, window)?;
            let device = Device::new(&instance, &surface)?;
            let win_size = window.inner_size();
            let fallback = vk::Extent2D {
                width: win_size.width.clamp(1, 16384),
                height: win_size.height.clamp(1, 16384),
            };
            let swapchain = Swapchain::new(&instance, &device, &surface, fallback)?;

            let mut allocator_create_info =
                AllocatorCreateInfo::new(&instance, &device, device.physical_device);
            allocator_create_info.flags = vk_mem::AllocatorCreateFlags::BUFFER_DEVICE_ADDRESS;

            let allocator = Allocator::new(allocator_create_info)?;

            let frames = (0..config.frames_in_flight)
                .map(|_| RenderFrame::new(&device))
                .collect::<Result<_, _>>()?;

            let descriptor_pool = {
                let pool_sizes = [
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::UNIFORM_BUFFER,
                        descriptor_count: 40960,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 40960,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::SAMPLED_IMAGE,
                        descriptor_count: 40960,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::SAMPLER,
                        descriptor_count: 40960,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: 40960,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 1024,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                        descriptor_count: 1024,
                    },
                ];

                let create_info = vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&pool_sizes)
                    .max_sets(100000)
                    .flags(
                        vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET
                            | vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND,
                    );

                device.create_descriptor_pool(&create_info, None).unwrap()
            };

            let immediate_command_buffer =
                Mutex::new(CommandBuffer::new(&device, device.universal_pool)?);

            let acceleration_structure_loader =
                ash::khr::acceleration_structure::Device::new(&instance, &device);
            let ray_tracing_pipeline_loader =
                ash::khr::ray_tracing_pipeline::Device::new(&instance, &device);

            Ok(Arc::new(Self {
                instance,
                surface,
                device,
                swapchain: Mutex::new(swapchain),
                frames,
                descriptor_pool,
                allocator,
                current_frame: Mutex::new(0),
                immediate_command_buffer,
                is_started: AtomicBool::new(false),
                acceleration_structure_loader,
                ray_tracing_pipeline_loader,
            }))
        }
    }

    pub fn is_started(&self) -> bool {
        self.is_started.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn recreate_swapchain(&self, fallback_extent: vk::Extent2D) -> anyhow::Result<()> {
        unsafe {
            self.device.device_wait_idle()?;

            let mut swc = self.swapchain.lock().unwrap();
            swc.destroy(&self.device);
            *swc = Swapchain::new(&self.instance, &self.device, &self.surface, fallback_extent)?;

            Ok(())
        }
    }

    pub fn get_current_frame_index(&self) -> usize {
        let current_frame = self.current_frame.lock().unwrap();

        *current_frame
    }

    /// Returns the number of frames in flight.
    pub fn frames_in_flight(&self) -> usize {
        self.frames.len()
    }

    pub fn begin_frame(&self) -> anyhow::Result<&RenderFrame> {
        self.is_started
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let frame = {
            let mut current_frame = self
                .current_frame
                .lock()
                .map_err(|e| anyhow!(e.to_string()))?;

            *current_frame = (*current_frame + 1) % self.frames.len();

            &self.frames[*current_frame]
        };

        unsafe {
            self.device.wait_for_fences(
                &[frame.render_command_buffer.submit_fence],
                true,
                u64::MAX,
            )?;
            self.device
                .reset_fences(&[frame.render_command_buffer.submit_fence])?;
        }

        frame.cleanup_resources(self);
        frame.render_command_buffer.reset(self)?;

        Ok(frame)
    }

    pub fn submit_and_present(
        &self,
        frame: &RenderFrame,
        swapchain_image: &SwapchainImage,
    ) -> anyhow::Result<()> {
        unsafe {
            self.device.submit(
                &frame.render_command_buffer,
                frame.swapchain_acquire_semaphore,
                swapchain_image.render_finished_semaphore,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags2::TRANSFER,
                vk::PipelineStageFlags2::ALL_GRAPHICS,
                self.device.universal_queue,
            )?;

            self.swapchain
                .lock()
                .unwrap()
                .present(&self.device, swapchain_image)?;
        }

        Ok(())
    }

    pub fn acquire_swapchain_image(&self, frame: &RenderFrame) -> anyhow::Result<SwapchainImage> {
        unsafe { self.swapchain.lock().unwrap().acquire_next_image(frame) }
    }

    pub(crate) fn schedule_resource_release(
        &self,
        resource: impl DeferDrop + Send + Sync + 'static,
    ) {
        let frame_index = self.get_current_frame_index();
        self.frames[frame_index].enqueue_drop(resource);
    }

    pub fn wait_idle(&self) -> anyhow::Result<()> {
        unsafe { self.device.device_wait_idle().map_err(Into::into) }
    }

    /// Query ray tracing pipeline properties (shader group handle size, alignment, etc.)
    pub fn ray_tracing_pipeline_properties(
        &self,
    ) -> vk::PhysicalDeviceRayTracingPipelinePropertiesKHR<'static> {
        let mut props = vk::PhysicalDeviceRayTracingPipelinePropertiesKHR::default();
        let mut props2 = vk::PhysicalDeviceProperties2::default().push_next(&mut props);
        unsafe {
            self.instance
                .get_physical_device_properties2(self.device.physical_device, &mut props2);
        }
        props
    }

    pub fn immediate_submit<F: FnOnce(&CommandBuffer)>(&self, f: F) -> anyhow::Result<()> {
        unsafe {
            let command_buffer = self.immediate_command_buffer.lock().unwrap();
            self.device.reset_fences(&[command_buffer.submit_fence])?;

            command_buffer.reset(self)?;
            command_buffer.begin(self)?;

            f(&command_buffer);

            command_buffer.end(self)?;

            let command_buffers = [vk::CommandBufferSubmitInfo::default()
                .command_buffer(command_buffer.command_buffer)];

            let submit_infos = [vk::SubmitInfo2::default().command_buffer_infos(&command_buffers)];

            self.device.queue_submit2(
                self.device.universal_queue,
                &submit_infos,
                command_buffer.submit_fence,
            )?;

            self.device
                .wait_for_fences(&[command_buffer.submit_fence], true, u64::MAX)?;

            Ok(())
        }
    }
}

impl Drop for RenderEngine {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            self.immediate_command_buffer
                .lock()
                .unwrap()
                .destroy(&self.device);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);

            for frame in &self.frames {
                frame.destroy(self);
            }

            self.swapchain.lock().unwrap().destroy(&self.device);
        }
    }
}
