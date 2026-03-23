use ash::vk::{
    PhysicalDevice, PhysicalDeviceFeatures, PhysicalDeviceProperties, PhysicalDeviceType,
    PresentModeKHR, SurfaceCapabilitiesKHR, SurfaceFormatKHR,
};

use crate::{instance::Instance, queue::queue_family::QueueFamily};

/// Represents physical device, used to make assumptions about it capabilities
/// 
///Readonly struct
#[derive(Clone)]
pub struct PDevice {
    instance: Instance,
    physical_device: PhysicalDevice,
}
impl PDevice {
    pub(crate) fn new(instance: Instance, physical_device: PhysicalDevice) -> Self {
        Self {
            instance,
            physical_device,
        }
    }
    ///Raw physical device handle -> `VKPhysicalDevice`
    pub fn handle(&self) -> PhysicalDevice {
        self.physical_device
    }
    ///Returns properties of physical device -> `VKPhysicalDeviceProperties`
    pub fn properties(&self) -> PhysicalDeviceProperties {
        unsafe {
            self.instance
                .instance()
                .get_physical_device_properties(self.physical_device)
        }
    }
    ///Returns features of physical device -> `VKPhysicalDeviceFeatures`
    pub fn features(&self) -> PhysicalDeviceFeatures {
        unsafe {
            self.instance
                .instance()
                .get_physical_device_features(self.physical_device)
        }
    }
    ///Returns 
    pub fn queue_families(&self) -> Vec<QueueFamily> {
        unsafe {
            self.instance
                .instance()
                .get_physical_device_queue_family_properties(self.physical_device)
        }
        .iter()
        .filter(|x| x.queue_count > 0)
        .enumerate()
        .map(|x| {
            let is_present = unsafe {
                self.instance
                    .surface_instance()
                    .get_physical_device_surface_support(
                        self.physical_device,
                        x.0 as u32,
                        self.instance.khr_surface(),
                    )
                    .expect("Couldn't check if device supports surface")
            };
            QueueFamily::new(x.0, x.1.clone(), is_present)
        })
        .collect()
    }
    /// Gives available surface present modes
    pub fn surface_present_modes(&self) -> Vec<PresentModeKHR> {
        unsafe {
            self.instance
                .surface_instance()
                .get_physical_device_surface_present_modes(
                    self.physical_device,
                    self.instance.khr_surface(),
                )
                .expect("Failed to acquire physical device present modes")
        }
    }
    /// Gives surface capabilities
    pub fn surface_capabilities(&self) -> SurfaceCapabilitiesKHR {
        unsafe {
            self.instance
                .surface_instance()
                .get_physical_device_surface_capabilities(
                    self.physical_device,
                    self.instance.khr_surface(),
                )
                .expect("Failed to acquire physical device present modes")
        }
    }
    /// Gives surface formats available
    pub fn surface_formats(&self) -> Vec<SurfaceFormatKHR> {
        unsafe {
            self.instance
                .surface_instance()
                .get_physical_device_surface_formats(
                    self.physical_device,
                    self.instance.khr_surface(),
                )
                .expect("Failed to acquire physical device formats")
        }
    }
    ///Returns unique queue family that is capable to as much as possible features (primarily PRESENT and GRAPHICS)
    ///
    /// Return None if no such queue exists
    pub fn universal_queue_family(&self) -> Option<QueueFamily> {
        if !self.is_present() || !self.queue_families().iter().any(|x| x.rate_unique() > 0) {
            return None;
        }
        self.queue_families()
            .iter()
            .filter(|x| x.is_present())
            .max_by(|x, x1| x.rate_unique().cmp(&x1.rate_unique()))
            .copied()
    }
    ///Returns two queue families one for present one for graphics
    ///
    ///Basically a fallback for unique queue family
    pub fn present_graphics_queue_family(&self) -> Option<(QueueFamily, QueueFamily)> {
        if !self.is_present() {
            return None;
        }
        Some((
            self.queue_families()
                .iter()
                .find(|x| x.is_present())
                .copied()?,
            self.queue_families()
                .iter()
                .find(|x| x.is_graphics())
                .copied()?,
        ))
    }
    /// Answers if this device can draw on screen
    pub fn is_present(&self) -> bool {
        self.queue_families().iter().any(|x| x.is_present())
    }

    ///Rates physical device based on it's capabilities
    ///
    ///Basically prefers discrete gpu that supports basic features
    ///
    ///Maybe I use somewhat similar with fn capability to determine what device to use.
    ///But for now this is it
    pub fn rate_default(&self) -> i32 {
        let properties = self.properties();
        let features = self.features();
        let mut score = 0;
        //Rating
        score += match properties.device_type {
            PhysicalDeviceType::DISCRETE_GPU => 1,
            _ => 0,
        };
        let requirements = [
            features.multi_viewport,
            features.geometry_shader,
            features.logic_op,
        ];
        for required in requirements {
            score += if required == ash::vk::TRUE { 1 } else { -1 }
        }

        score
    }
}
