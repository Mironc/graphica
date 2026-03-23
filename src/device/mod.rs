//! This module contains everything related to graphics devices
//! 
//! Includes three structs:
//! - `PDevice` - this structure is used to choose suitable rendering device  
//! - `DeviceContext` - linked to choosen `PDevice`, gives access to all device specific commands 
//! - `GraphicsQueue` - linked to `DeviceContext`, its specialized for graphics and present commands



mod device_context;
mod graphics_queue;
mod physical_device;



pub use device_context::*;
pub use graphics_queue::*;
pub use physical_device::*;
