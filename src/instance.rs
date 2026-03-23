use std::{
    error::Error,
    ffi::{CStr, CString, c_void},
    ops::Deref,
};

use ash::{
    Entry, Instance as I,
    ext::debug_utils,
    khr::surface::Instance as SI,
    vk::{self, SurfaceKHR},
};
use ash_window::enumerate_required_extensions;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::device::PDevice;
#[derive(Clone)]
pub struct Instance {
    instance: I,
    _debug_utils: debug_utils::Instance,
    _debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    surface_instance: SI,
    surface_khr: SurfaceKHR,
}
impl Instance {
    pub fn init(entry: &Entry, window: &Window) -> Result<Self, Box<dyn Error>> {
        for layer in unsafe { entry.enumerate_instance_layer_properties() }.unwrap() {
            println!("{:?}", layer)
        }
        // Supported vulkan version
        let (major, minor) = match unsafe { entry.try_enumerate_instance_version()? } {
            // Vulkan 1.1+
            Some(version) => (
                vk::api_version_major(version),
                vk::api_version_minor(version),
            ),
            // Vulkan 1.0
            None => (1, 0),
        };
        log::info!("Vulkan {major}.{minor} supported");
        // Vulkan instance
        let app_name = CString::new(" ")?;
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name.as_c_str())
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(c"No Engine")
            .engine_version(vk::make_api_version(0, 0, 1, 0))
            .api_version(vk::make_api_version(0, 1, 1, 0));

        let mut extension_names =
            enumerate_required_extensions(window.display_handle()?.as_raw())?.to_vec();
        extension_names.push(debug_utils::NAME.as_ptr());

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
        }

        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };

        let layer_names = [std::ffi::CString::new("VK_LAYER_KHRONOS_validation").unwrap()];
        //let layer_names: [CString; 0] = [];
        let layers_pointers: Vec<*const i8> = layer_names
            .iter()
            .map(|layer_name| layer_name.as_ptr())
            .collect();

        let instance_create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .flags(create_flags)
            .enabled_layer_names(&layers_pointers);

        let instance = unsafe { entry.create_instance(&instance_create_info, None)? };
        let surface_instance = SI::new(entry, &instance);

        let surface = unsafe {
            ash_window::create_surface(
                entry,
                &instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )?
        };
        // Vulkan debug report
        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .flags(vk::DebugUtilsMessengerCreateFlagsEXT::empty())
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));
        let debug_utils = debug_utils::Instance::new(entry, &instance);
        let debug_utils_messenger =
            unsafe { debug_utils.create_debug_utils_messenger(&create_info, None)? };

        Ok(Self {
            instance,
            _debug_utils: debug_utils,
            _debug_utils_messenger: debug_utils_messenger,
            surface_instance,
            surface_khr: surface,
        })
    }
    pub fn khr_surface(&self) -> SurfaceKHR {
        self.surface_khr
    }
    pub fn surface_instance(&self) -> &SI {
        &self.surface_instance
    }
    pub fn list_devices(&self) -> Result<Vec<PDevice>, Box<dyn Error>> {
        let physical_devices = unsafe { self.instance.enumerate_physical_devices()? };
        let graphic_devices: Vec<PDevice> = physical_devices
            .iter()
            .map(|x| PDevice::new(self.clone(), *x))
            .collect();
        graphic_devices.iter().for_each(|x| {
            x.queue_families();
        });
        Ok(graphic_devices)
    }
    pub fn instance(&self) -> &I {
        &self.instance
    }
}
impl Deref for Instance {
    type Target = I;

    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    flag: vk::DebugUtilsMessageSeverityFlagsEXT,
    typ: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    use vk::DebugUtilsMessageSeverityFlagsEXT as Flag;

    let message = unsafe { CStr::from_ptr((*p_callback_data).p_message) };
    match flag {
        Flag::VERBOSE => log::debug!("{:?} - {:?}", typ, message),
        Flag::INFO => log::info!("{:?} - {:?}", typ, message),
        Flag::WARNING => log::warn!("{:?} - {:?}", typ, message),
        _ => log::error!("{:?} - {:?}", typ, message),
    }
    vk::FALSE
}
