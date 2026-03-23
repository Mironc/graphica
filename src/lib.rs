//! This library purpose is to provide capabilities to draw effectively using vulkan renderer with implementation of frame graph.
//!
//!
//! It's hardly bond to vulkan and other graphics APIs aren't anywhere in view
//!
//!
pub mod context;
pub mod device;
pub mod instance;
pub mod queue;
pub mod render_graph;
pub mod rendering;
pub mod swapchain;

extern crate self as graphics;
