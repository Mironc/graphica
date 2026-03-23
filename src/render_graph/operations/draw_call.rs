use ash::vk::{
    self, AccessFlags, ClearColorValue, ClearValue, CommandBuffer, DescriptorBufferInfo, Extent2D,
    ImageLayout, PipelineBindPoint, PipelineStageFlags, RenderPassBeginInfo, ShaderStageFlags,
    SubpassContents, WriteDescriptorSet,
};
use log::warn;

use crate::{
    device::DeviceContext,
    render_graph::{
        render_graph::{ResourceAccess, ResourceState, ResourceUsage},
        resource::ResourceId,
    },
    rendering::{
        buffer_container::{
            GeneralBufferId, IndexBuffer, IndexBufferId, IndexData, VertexBuffer, VertexBufferId,
            VertexData,
        },
        descriptor_container::{BindedRes, DescriptorId, RawDescriptorId},
        framebuffer_container::FramebufferId,
        pipeline_container::PipelineId,
        render_pass_container::AttachmentUsage,
        renderer_bundle::RendererBundle,
        shader_container::{DescriptorBinding, PushWriter},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCall {
    Direct { draw_param: DrawParameters },
    // TODO: Add more variants such as Indirect, Instanced
}

impl DrawCall {
    pub fn execute(
        &self,
        bundle: &RendererBundle,
        command_buffer: CommandBuffer,
        device: &DeviceContext,
    ) {
        if let None = match self {
            DrawCall::Direct { draw_param } => draw_param.execute(bundle, command_buffer, device),
        } {
            warn!("Draw call went wrong")
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawGeometry {
    Buffered {
        vbo: GeneralBufferId,
        ebo: Option<GeneralBufferId>,
    },
    Procedural {
        count: u32,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawParameters {
    geometry: DrawGeometry,
    framebuffer: FramebufferId,
    pipeline: PipelineId,
    push_buf: Option<[u8; 128]>,
    desc: Option<DescriptorId>,
}
impl DrawParameters {
    pub fn new(
        geometry: DrawGeometry,
        framebuffer: FramebufferId,
        pipeline: PipelineId,
        push_writer: Option<&PushWriter>,
        desc: Option<DescriptorId>,
    ) -> Self {
        Self {
            geometry,
            framebuffer,
            pipeline,
            push_buf: push_writer.map(|x| x.buf()),
            desc,
        }
    }

    pub fn resource_state(&self, bundle: &RendererBundle) -> Option<Vec<ResourceState>> {
        let pipeline = bundle.pipeline_container.get(self.pipeline)?;
        let framebuffer = bundle
            .framebuffer_container
            .get_framebuffer(self.framebuffer)?;
        let view_ids = framebuffer.views_id();
        let render_pass = pipeline.render_pass();
        let mut resource_state = Vec::new();

        // renderpass attachments
        {
            for &read_id in render_pass.description().subpass.read_attachments() {
                let attachment = pipeline
                    .render_pass()
                    .description()
                    .attachments
                    .get(read_id)?;
                let image_view = *view_ids.get(read_id)?;
                let resource_id = ResourceId::Texture(image_view.texture());
                let ps = if attachment.format.is_color() {
                    PipelineStageFlags::FRAGMENT_SHADER
                } else {
                    PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | PipelineStageFlags::LATE_FRAGMENT_TESTS
                };
                resource_state.push(ResourceState::new(
                    resource_id,
                    ResourceUsage::TextureTranstional(
                        attachment.initial_layout,
                        attachment.final_layout,
                        ps,
                        attachment.access_flags(AttachmentUsage::Read),
                        ResourceAccess::Read,
                    ),
                ));
            }
            for &write_id in render_pass.description().subpass.write_attachments() {
                let attachment = pipeline
                    .render_pass()
                    .description()
                    .attachments
                    .get(write_id)?;
                let image_view = *view_ids.get(write_id)?;
                let resource_id = ResourceId::Texture(image_view.texture());
                let ps = if attachment.format.is_color() {
                    PipelineStageFlags::FRAGMENT_SHADER
                        | PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                } else {
                    PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | PipelineStageFlags::LATE_FRAGMENT_TESTS
                };
                resource_state.push(ResourceState::new(
                    resource_id,
                    ResourceUsage::TextureTranstional(
                        attachment.initial_layout,
                        attachment.final_layout,
                        ps,
                        attachment.access_flags(AttachmentUsage::Write),
                        ResourceAccess::Write,
                    ),
                ));
            }
            for &rw_id in render_pass.description().subpass.rw_attachments() {
                let attachment = pipeline
                    .render_pass()
                    .description()
                    .attachments
                    .get(rw_id)?;
                let image_view = *view_ids.get(rw_id)?;
                let resource_id = ResourceId::Texture(image_view.texture());
                let ps = if attachment.format.is_color() {
                    PipelineStageFlags::FRAGMENT_SHADER
                } else {
                    PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | PipelineStageFlags::LATE_FRAGMENT_TESTS
                };
                resource_state.push(ResourceState::new(
                    resource_id,
                    ResourceUsage::TextureTranstional(
                        attachment.initial_layout,
                        attachment.final_layout,
                        ps,
                        attachment.access_flags(AttachmentUsage::ReadWrite),
                        ResourceAccess::ReadWrite,
                    ),
                ));
            }
        }

        fn shader_to_pipeline_stage(shader_flags: ShaderStageFlags) -> PipelineStageFlags {
            let mut pipeline_flags = PipelineStageFlags::empty();
            if shader_flags.contains(ShaderStageFlags::VERTEX) {
                pipeline_flags |= PipelineStageFlags::VERTEX_SHADER;
            }
            if shader_flags.contains(ShaderStageFlags::FRAGMENT) {
                pipeline_flags |= PipelineStageFlags::FRAGMENT_SHADER;
            }
            if shader_flags.contains(ShaderStageFlags::GEOMETRY) {
                pipeline_flags |= PipelineStageFlags::GEOMETRY_SHADER;
            }

            pipeline_flags
        }

        // descriptor sets write
        if let Some(desc) = &self.desc {
            if let Some(descriptor_sets) = bundle.descriptor_container.get_descriptor(desc.id()) {
                for (desc_bind, bind) in descriptor_sets.binded().iter() {
                    let res_state = match bind {
                        BindedRes::Buffer(general_buffer_id, offset, size) => {
                            let bind_point = shader_to_pipeline_stage(desc_bind.stage_flags);
                            ResourceState::new(
                                ResourceId::Buffer(*general_buffer_id),
                                ResourceUsage::Buffer(
                                    bind_point,
                                    *offset,
                                    *size,
                                    AccessFlags::SHADER_READ,
                                    ResourceAccess::Read,
                                ), //as far as I know they cannot be changed in shaders
                            )
                        }
                        BindedRes::Texture(texture_view_id) => {
                            let bind_point = shader_to_pipeline_stage(desc_bind.stage_flags);
                            ResourceState::new(
                                ResourceId::Texture(texture_view_id.texture()),
                                ResourceUsage::Texture(
                                    ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                                    bind_point,
                                    AccessFlags::SHADER_READ,
                                    ResourceAccess::Read,
                                ),
                            )
                        }
                        BindedRes::Sampler(_) => continue,
                    };
                    resource_state.push(res_state);
                }
            }
        }

        if let DrawGeometry::Buffered { vbo, ebo } = self.geometry {
            resource_state.push(ResourceState::new(
                ResourceId::Buffer(vbo),
                ResourceUsage::Buffer(
                    PipelineStageFlags::VERTEX_INPUT,
                    0,
                    vbo.item_size() * vbo.len(),
                    AccessFlags::VERTEX_ATTRIBUTE_READ,
                    ResourceAccess::Read,
                ),
            ));
            if let Some(ebo) = ebo {
                resource_state.push(ResourceState::new(
                    ResourceId::Buffer(ebo),
                    ResourceUsage::Buffer(
                        PipelineStageFlags::VERTEX_INPUT,
                        0,
                        ebo.item_size() * ebo.item_size(),
                        AccessFlags::INDEX_READ,
                        ResourceAccess::Read,
                    ),
                ));
            }
        }
        Some(resource_state)
    }
    pub fn execute(
        &self,
        bundle: &RendererBundle,
        command_buffer: CommandBuffer,
        device: &DeviceContext,
    ) -> Option<()> {
        let pipeline = bundle.pipeline_container.get(self.pipeline)?;
        let framebuffer = bundle
            .framebuffer_container
            .get_framebuffer(self.framebuffer)?;
        let view_ids = framebuffer.views_id();
        let render_pass = pipeline.render_pass();
        let viewports = [ash::vk::Viewport::default()
            .width(framebuffer.width() as f32)
            .height(framebuffer.height() as f32)
            .x(0.0)
            .y(0.0)];
        let extent = Extent2D::default()
            .height(framebuffer.height())
            .width(framebuffer.width());
        unsafe {
            device.cmd_set_viewport(command_buffer, 0, &viewports);

            let scissors = [ash::vk::Rect2D::from(extent)];
            device.cmd_set_scissor(command_buffer, 0, &scissors);
            let clear_values = [ClearValue {
                color: ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_pass_begin = RenderPassBeginInfo::default()
                .framebuffer(framebuffer.handle())
                .render_pass(render_pass.handle())
                .render_area(ash::vk::Rect2D::from(extent))
                .clear_values(&clear_values);
            device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_begin,
                SubpassContents::INLINE,
            );
            device.cmd_bind_pipeline(
                command_buffer,
                PipelineBindPoint::GRAPHICS,
                pipeline.handle(),
            );
            // push constant update
            if let Some(push_buf) = self.push_buf {
                pipeline.pipeline_layout();
                let mut stages = ash::vk::ShaderStageFlags::empty();
                for s in pipeline
                    .pipeline_layout()
                    .shader_layout()
                    .push_constants
                    .iter()
                {
                    stages |= s.stage_flags;
                }
                device.cmd_push_constants(
                    command_buffer,
                    pipeline.pipeline_layout().pipeline_layout(),
                    stages,
                    0,
                    &push_buf,
                );
            }
            if let Some(desc_id) = &self.desc {
                if let Some(descriptor) = bundle.descriptor_container.get_descriptor(desc_id.id()) {
                    if !descriptor.binded().is_empty() {
                        device.cmd_bind_descriptor_sets(
                            command_buffer,
                            PipelineBindPoint::GRAPHICS,
                            pipeline.pipeline_layout().pipeline_layout(),
                            0,
                            &descriptor.handles(),
                            &[],
                        );
                    }
                }
            }
            match self.geometry {
                DrawGeometry::Buffered { vbo, ebo } => {
                    let vertex_buffer = bundle.buffer_container.get_general_buffer(vbo)?;
                    let index_buffer =
                        ebo.and_then(|x| bundle.buffer_container.get_general_buffer(x));

                    let buffers = [vertex_buffer.handle()];
                    let offsets = [0];
                    device.cmd_bind_vertex_buffers(command_buffer, 0, &buffers, &offsets);
                    if let Some(index_buffer) = index_buffer {
                        todo!("Add indexed drawing")
                    } else {
                        // TODO:Add first vertex parameter (for the sake of bindless resource impl)
                        device.cmd_draw(command_buffer, vbo.len() as u32, 1, 0, 0);
                    }
                }
                DrawGeometry::Procedural { count } => {
                    device.cmd_draw(command_buffer, count, 1, 0, 0)
                }
            }
            device.cmd_end_render_pass(command_buffer);
        }
        Some(())
    }
}

impl DrawParameters {}
