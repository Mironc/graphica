use ash::Entry;
use std::error::Error;
use winit::window::Window;

use crate::instance::Instance;

pub struct GraphicsContext {
    entry: Entry,
    instance: Instance,
}
impl GraphicsContext {
    pub fn init(window: &Window) -> Result<Self, Box<dyn Error>> {
        let entry = unsafe { Entry::load()? };
        let instance = Instance::init(&entry, window)?;
        Ok(Self { entry, instance })
    }
    pub fn instance(&self) -> &Instance {
        &self.instance
    }
}
/* impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
            self.instance
                .surface_instance()
                .destroy_surface(self.instance.khr_surface(), None);
            self.instance.destroy_instance(None);
        }
    }
} */
