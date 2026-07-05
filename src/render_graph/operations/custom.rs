use std::fmt::Debug;

use ash::vk::CommandBuffer;
use dyn_clone::DynClone;

use crate::{
    device::DeviceContext, render_graph::resource_state::ResourceState,
    rendering::renderer_bundle::RendererBundle,
};

pub trait CustomOp: Debug + DynClone {
    fn execute(
        &self,
        device_context: &DeviceContext,
        command_buffer: CommandBuffer,
        bundle: &mut RendererBundle,
    );
    fn resource_state(&self, bundle: &RendererBundle) -> Option<Vec<ResourceState>>;
}
dyn_clone::clone_trait_object!(CustomOp);
