use ash::ext::debug_utils;
use ash::vk::DebugUtilsMessengerEXT;
use ash::{vk, Entry};
use std::borrow::Cow;
use std::ffi;
use std::ffi::c_char;
use std::ops::Deref;
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::Window;

pub struct Instance {
    instance: ash::Instance,
    pub entry: Entry,
    debug_utils_loader: debug_utils::Instance,
    debug_callback: DebugUtilsMessengerEXT,
}

impl Instance {
    pub unsafe fn new(window: &Window, debug: bool) -> anyhow::Result<Self> {
        let entry = ash::Entry::load()?;
        let instance = {
            let app_info = vk::ApplicationInfo::default()
                .engine_name(c"Lantir Engine")
                .application_name(c"Lantir App")
                .application_version(vk::make_api_version(0, 1, 0, 0))
                .engine_version(vk::make_api_version(0, 1, 0, 0))
                .api_version(vk::make_api_version(0, 1, 3, 0));

            let create_flags = vk::InstanceCreateFlags::default();

            let enabled_layers = get_enabled_layers();
            let enabled_extensions = get_enabled_extensions(&window, debug)?;

            let create_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_layer_names(&enabled_layers)
                .enabled_extension_names(&enabled_extensions)
                .flags(create_flags);

            entry.create_instance(&create_info, None)?
        };

        let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));

        let debug_utils_loader = debug_utils::Instance::new(&entry, &instance);
        let debug_callback = debug_utils_loader.create_debug_utils_messenger(&debug_info, None)?;

        Ok(Instance {
            instance,
            entry: entry,
            debug_utils_loader,
            debug_callback,
        })
    }

    pub unsafe fn destroy(&mut self) {
        self.debug_utils_loader
            .destroy_debug_utils_messenger(self.debug_callback, None);
        self.instance.destroy_instance(None);
    }
}

impl Deref for Instance {
    type Target = ash::Instance;
    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        ffi::CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    if message_severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
        println!(
            "{message_severity:?}:\n{message_type:?} [{message_id_name} ({message_id_number})] : {message}\n",
        );
    }

    vk::FALSE
}

fn get_enabled_layers() -> Vec<*const c_char> {
    let enabled_layers = [c"VK_LAYER_KHRONOS_validation"];
    enabled_layers
        .iter()
        .map(|raw_name| raw_name.as_ptr())
        .collect()
}

fn get_enabled_extensions(window: &Window, debug: bool) -> anyhow::Result<Vec<*const c_char>> {
    let mut res = ash_window::enumerate_required_extensions(
        window
            .display_handle()
            .expect("Failed to get winow handle")
            .as_raw(),
    )?
    .to_vec();

    if debug {
        res.push(debug_utils::NAME.as_ptr());
    }

    Ok(res)
}
