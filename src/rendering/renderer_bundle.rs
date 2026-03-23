use crate::{
    device::DeviceContext,
    render_graph::{render_graph::ResourceState, resource::ResourceId},
    rendering::{
        buffer_container::BufferContainer, descriptor_container::DescriptorContainer,
        framebuffer_container::FramebufferContainer, pipeline_container::PipelineContainer,
        render_pass_container::RenderPassContainer, shader_container::ShaderContainer,
        state_container::StateContainer, texture_container::TextureContainer,
    },
    swapchain::{FrameData, FrameImage},
};
#[derive(Default)]
pub struct RendererBundle {
    pub texture_container: TextureContainer,
    pub framebuffer_container: FramebufferContainer,
    pub shader_container: ShaderContainer,
    pub buffer_container: BufferContainer,
    pub render_pass_container: RenderPassContainer,
    pub pipeline_container: PipelineContainer,
    pub descriptor_container: DescriptorContainer,
    pub(crate) resource_state: StateContainer,
}
impl RendererBundle {
    pub fn new() -> Self {
        Self {
            texture_container: TextureContainer::default(),
            framebuffer_container: FramebufferContainer::default(),
            shader_container: ShaderContainer::default(),
            buffer_container: BufferContainer::default(),
            render_pass_container: RenderPassContainer::default(),
            descriptor_container: DescriptorContainer::default(),
            pipeline_container: PipelineContainer::default(),
            resource_state: StateContainer::default(),
        }
    }
    pub fn remove_frameimage(&mut self, device: &DeviceContext, frame_image: &FrameImage) {
        if let Some(ids) = self.texture_container.remove_frameimage(frame_image) {
            self.framebuffer_container.delete_image_view(device, ids.1);
            self.resource_state.remove(ResourceId::Texture(ids.0));
        }
    }
}
