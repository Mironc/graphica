use crate::{
    device::DeviceContext,
    render_graph::resource::ResourceId,
    rendering::{
        buffer_container::BufferContainer, descriptor_container::DescriptorContainer,
        framebuffer_container::FramebufferContainer, label_container::LabelContainer,
        pass_container::PassContainer, shader_container::ShaderContainer,
        state_container::StateContainer, texture_container::TextureContainer,
    },
    swapchain::FrameImage,
};
#[derive(Default)]
pub struct RendererBundle {
    pub texture_container: TextureContainer,
    pub framebuffer_container: FramebufferContainer,
    pub shader_container: ShaderContainer,
    pub buffer_container: BufferContainer,
    pub pass_container: PassContainer,
    pub descriptor_container: DescriptorContainer,
    pub label_container: LabelContainer,
    pub(crate) resource_state: StateContainer,
}
impl RendererBundle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn remove_frameimage(&mut self, device: &DeviceContext, frame_image: &FrameImage) {
        if let Some(ids) = self.texture_container.remove_frameimage(frame_image) {
            self.framebuffer_container.delete_image_view(device, ids.1);
            self.resource_state.remove(ResourceId::Texture(ids.0));
        }
    }
}
