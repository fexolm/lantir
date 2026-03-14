use crate::command_buffer::CommandBuffer;
use crate::instance::Instance;
use crate::surface::Surface;
use ash::khr::swapchain;
use ash::vk;
use std::ffi::{CStr, c_void};
use std::ops::Deref;

pub(crate) struct Device {
    device: ash::Device,
    pub(crate) physical_device: vk::PhysicalDevice,

    pub(crate) universal_family: u32,
    pub(crate) universal_queue: vk::Queue,
    pub(crate) universal_pool: vk::CommandPool,
}

impl Device {
    pub(crate) unsafe fn new(instance: &Instance, surface: &Surface) -> anyhow::Result<Self> {
        let SelectedPhysicalDevice {
            physical_device,
            universal_family,
        } = select_physical_device(instance, surface)?;

        let device = {
            let device_extension_names_raw = [
                swapchain::NAME.as_ptr(),
                vk::KHR_SHADER_NON_SEMANTIC_INFO_NAME.as_ptr(),
                vk::KHR_ACCELERATION_STRUCTURE_NAME.as_ptr(),
                vk::KHR_RAY_TRACING_PIPELINE_NAME.as_ptr(),
                vk::KHR_DEFERRED_HOST_OPERATIONS_NAME.as_ptr(),
            ];

            let features = vk::PhysicalDeviceFeatures {
                shader_clip_distance: 1,
                shader_sampled_image_array_dynamic_indexing: 1,
                multi_draw_indirect: 1,
                draw_indirect_first_instance: 1,
                ..Default::default()
            };

            let features2 = vk::PhysicalDeviceFeatures2::default().features(features);
            let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
                .descriptor_indexing(true)
                .buffer_device_address(true);
            let mut features13 = vk::PhysicalDeviceVulkan13Features::default()
                .synchronization2(true)
                .shader_demote_to_helper_invocation(true)
                .dynamic_rendering(true);
            let mut as_features = vk::PhysicalDeviceAccelerationStructureFeaturesKHR::default()
                .acceleration_structure(true);
            let mut rtp_features =
                vk::PhysicalDeviceRayTracingPipelineFeaturesKHR::default()
                    .ray_tracing_pipeline(true);

            let mut all_features = features2
                .push_next(&mut features12)
                .push_next(&mut features13)
                .push_next(&mut as_features)
                .push_next(&mut rtp_features);

            let priorities = [1.0];

            let queue_infos = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(universal_family)
                .queue_priorities(&priorities)];

            let create_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_infos)
                .enabled_extension_names(&device_extension_names_raw)
                .push_next(&mut all_features);
            instance
                .create_device(physical_device, &create_info, None)
                .unwrap()
        };

        let universal_queue = device.get_device_queue(universal_family, 0);

        let universal_pool = {
            let create_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(universal_family);
            device.create_command_pool(&create_info, None)?
        };

        Ok(Device {
            device,
            physical_device,
            universal_family,
            universal_queue,
            universal_pool,
        })
    }

    pub(crate) unsafe fn submit(
        &self,
        cb: &CommandBuffer,
        wait_semaphore: vk::Semaphore,
        signal_semaphore: vk::Semaphore,
        wait_stage: vk::PipelineStageFlags2,
        signal_stage: vk::PipelineStageFlags2,
        queue: vk::Queue,
    ) -> anyhow::Result<()> {
        let wait_semaphores = [vk::SemaphoreSubmitInfo::default()
            .semaphore(wait_semaphore)
            .stage_mask(wait_stage)
            .value(1)
            .device_index(0)];

        let signal_semaphores = [vk::SemaphoreSubmitInfo::default()
            .semaphore(signal_semaphore)
            .stage_mask(signal_stage)
            .value(1)
            .device_index(0)];

        let command_buffer_infos = [vk::CommandBufferSubmitInfo::default()
            .command_buffer(cb.command_buffer)
            .device_mask(0)];

        let submit_info = vk::SubmitInfo2::default()
            .wait_semaphore_infos(&wait_semaphores)
            .signal_semaphore_infos(&signal_semaphores)
            .command_buffer_infos(&command_buffer_infos);

        self.device
            .queue_submit2(queue, &[submit_info], cb.submit_fence)?;

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.universal_pool, None);
            self.device.destroy_device(None);
        }
    }
}

impl Deref for Device {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

struct SelectedPhysicalDevice {
    physical_device: vk::PhysicalDevice,
    universal_family: u32,
}

fn get_required_device_extensions() -> [&'static CStr; 4] {
    [
        swapchain::NAME,
        vk::KHR_ACCELERATION_STRUCTURE_NAME,
        vk::KHR_RAY_TRACING_PIPELINE_NAME,
        vk::KHR_DEFERRED_HOST_OPERATIONS_NAME,
    ]
}

fn check_required_extensions(instance: &ash::Instance, device: vk::PhysicalDevice) -> bool {
    let required_extentions = get_required_device_extensions();

    let extension_props = unsafe {
        instance
            .enumerate_device_extension_properties(device)
            .unwrap()
    };

    for required in required_extentions.iter() {
        let found = extension_props.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            required == &name
        });

        if !found {
            return false;
        }
    }

    true
}

fn check_required_features(instance: &ash::Instance, device: vk::PhysicalDevice) -> bool {
    let features = unsafe { instance.get_physical_device_features(device) };
    let mut features2 = vk::PhysicalDeviceFeatures2::default();
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features13 = vk::PhysicalDeviceVulkan13Features::default();
    features2.p_next = &mut features12 as *mut _ as *mut c_void;
    features12.p_next = &mut features13 as *mut _ as *mut c_void;

    unsafe { instance.get_physical_device_features2(device, &mut features2) };

    features.sampler_anisotropy == vk::TRUE
        && features12.buffer_device_address == vk::TRUE
        && features12.descriptor_indexing == vk::TRUE
        && features13.dynamic_rendering == vk::TRUE
        && features13.synchronization2 == vk::TRUE
}

unsafe fn find_universal_queue_family(
    instance: &ash::Instance,
    surface: &Surface,
    device: vk::PhysicalDevice,
) -> Option<u32> {
    unsafe {
        let props = instance.get_physical_device_queue_family_properties(device);

        for (idx, family) in props.iter().enumerate() {
            let idx = idx as u32;

            if !family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                continue;
            }

            let present_support = surface
                .get_physical_device_surface_support(device, idx, surface.get_raw())
                .unwrap();

            if present_support {
                return Some(idx);
            }
        }

        None
    }
}

unsafe fn select_physical_device(
    instance: &ash::Instance,
    surface: &Surface,
) -> anyhow::Result<SelectedPhysicalDevice> {
    let devices = instance.enumerate_physical_devices()?;

    Ok(devices
        .iter()
        .find_map(|&physical_device| {
            if !check_required_extensions(instance, physical_device)
                || !check_required_features(instance, physical_device)
            {
                return None;
            }

            find_universal_queue_family(instance, surface, physical_device).map(
                |universal_family| SelectedPhysicalDevice {
                    physical_device,
                    universal_family,
                },
            )
        })
        .expect("Couldn't find suitable device."))
}
