use ash::vk::{
    AccessFlags, ClearColorValue, ClearValue, CommandBuffer, Extent2D, ImageLayout,
    PipelineBindPoint, PipelineStageFlags, RenderPassBeginInfo, ShaderStageFlags, SubpassContents,
};
use log::warn;

use crate::{
    device::DeviceContext,
    render_graph::{
        resource::ResourceId,
        resource_state::{ResourceAccess, ResourceState, ResourceUsage},
    },
    rendering::{
        buffer_container::GeneralBufferId,
        descriptor_container::{BindedRes, DescriptorWriter},
        framebuffer_container::FramebufferId,
        pass_container::PassId,
        renderer_bundle::RendererBundle,
        shader_container::{PushWriter, ShaderType},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCall {
    Direct { draw_param: DrawData },
    // TODO: Add more variants such as Indirect, Instanced
}
impl DrawCall {
    pub fn execute(
        &self,
        bundle: &mut RendererBundle,
        command_buffer: CommandBuffer,
        device: &DeviceContext,
    ) {
        if (match self {
            DrawCall::Direct { draw_param } => draw_param.execute(bundle, command_buffer, device),
        })
        .is_none()
        {
            warn!("Draw call went wrong")
        }
    }
    pub fn draw_data(&self) -> &DrawData {
        match self {
            DrawCall::Direct { draw_param } => draw_param,
        }
    }
    pub fn draw_pass(&self) -> &DrawPass {
        match self {
            DrawCall::Direct { draw_param } => &draw_param.pass,
        }
    }
    fn draw_pass_mut(&mut self) -> &mut DrawPass {
        match self {
            DrawCall::Direct { draw_param } => &mut draw_param.pass,
        }
    }
    pub fn insert_sync(&mut self, transitions: Vec<(ResourceState, Option<ResourceState>)>) {
        self.draw_pass_mut().make_synced(transitions);
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
pub enum DrawPass {
    Synced(PassId, Vec<(ResourceState, Option<ResourceState>)>),
    Preparing(PassId),
}
impl DrawPass {
    pub fn pass_id(&self) -> PassId {
        *match self {
            DrawPass::Synced(pass_id, _) | DrawPass::Preparing(pass_id) => pass_id,
        }
    }
    pub fn make_synced(&mut self, transitions: Vec<(ResourceState, Option<ResourceState>)>) {
        *self = Self::Synced(self.pass_id(), transitions);
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawData {
    geometry: DrawGeometry,
    framebuffer: FramebufferId,
    pass: DrawPass,
    push_buf: Option<[u8; 128]>,
    desc: Option<DescriptorWriter>,
}
impl DrawData {
    pub fn new(
        geometry: DrawGeometry,
        framebuffer: FramebufferId,
        pass: PassId,
        push_writer: Option<&PushWriter>,
        desc: Option<DescriptorWriter>,
    ) -> Self {
        Self {
            geometry,
            framebuffer,
            pass: DrawPass::Preparing(pass),
            push_buf: push_writer.map(|x| x.buf()),
            desc,
        }
    }

    pub fn resource_state(&self, bundle: &RendererBundle) -> Option<Vec<ResourceState>> {
        let view_ids = bundle
            .framebuffer_container
            .get_framebuffer_layout(self.framebuffer)?;
        let mut resource_state = Vec::new();

        // renderpass attachments
        let layout = bundle.pass_container.get_pass(self.pass.pass_id())?;

        for (&(_, index), _) in layout
            .shader_layout()
            .output_types()
            .iter()
            .filter(|x| x.0.0 == ShaderType::Fragment)
        {
            let view_id = view_ids.get(index as usize)?;
            let texture = bundle.texture_container.get_image(view_id.texture())?;
            let resource_id = ResourceId::Texture(view_id.texture());

            let ps = if texture.texture_format().is_color() {
                PipelineStageFlags::FRAGMENT_SHADER | PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            } else {
                PipelineStageFlags::EARLY_FRAGMENT_TESTS | PipelineStageFlags::LATE_FRAGMENT_TESTS
            };

            let image_layout = if texture.texture_format().is_color() {
                ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            } else if texture.texture_format().is_depth() {
                ImageLayout::DEPTH_ATTACHMENT_OPTIMAL
            } else {
                ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
            };

            let access_flags = if texture.texture_format().is_depth() {
                AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
            } else {
                AccessFlags::COLOR_ATTACHMENT_WRITE | AccessFlags::COLOR_ATTACHMENT_READ
            };

            match &self.pass {
                DrawPass::Synced(_, items) => {
                    if let Some(twins) = items.iter().find(|x| x.0.resource_id() == resource_id) {
                        let (initial, final_) = (
                            twins.0.resource_usage(),
                            twins.1.map(|x| x.resource_usage()).unwrap_or_else(|| {
                                ResourceUsage::Texture(
                                    image_layout,
                                    ps,
                                    access_flags,
                                    ResourceAccess::Write,
                                )
                            }),
                        );
                        if let (
                            ResourceUsage::Texture(
                                image_layout,
                                pipeline_stage_flags,
                                access_flags,
                                _,
                            )
                            | ResourceUsage::TextureTranstional(
                                _,
                                image_layout,
                                _,
                                pipeline_stage_flags,
                                _,
                                access_flags,
                                _,
                            ),
                            ResourceUsage::Texture(
                                image_layout_1,
                                pipeline_stage_flags_1,
                                access_flags_1,
                                _,
                            )
                            | ResourceUsage::TextureTranstional(
                                image_layout_1,
                                _,
                                pipeline_stage_flags_1,
                                _,
                                access_flags_1,
                                _,
                                _,
                            ),
                        ) = (initial, final_)
                        {
                            let usage = ResourceUsage::TextureTranstional(
                                image_layout,
                                image_layout_1,
                                pipeline_stage_flags,
                                pipeline_stage_flags_1,
                                access_flags,
                                access_flags_1,
                                ResourceAccess::Read,
                            );
                            resource_state.push(ResourceState::new(resource_id, usage));
                        } else {
                            log::error!("For some reason its not synced as texture")
                        }
                    } else {
                        log::error!("No transition for index {} is found in synced", index);
                    }
                }
                DrawPass::Preparing(_) => {
                    resource_state.push(ResourceState::new(
                        resource_id,
                        ResourceUsage::Texture(
                            image_layout,
                            ps,
                            access_flags,
                            ResourceAccess::Write,
                        ),
                    ));
                }
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
        if let Some(writer) = &self.desc
            && let Ok(binded) = bundle.descriptor_container.binded_res(
                writer,
                &bundle.pass_container,
                self.pass.pass_id(),
            )
        {
            for (desc_bind, bind) in binded.iter() {
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
        bundle: &mut RendererBundle,
        command_buffer: CommandBuffer,
        device: &DeviceContext,
    ) -> Option<()> {
        let pass = {
            let attachments = bundle
                .framebuffer_container
                .get_framebuffer_layout(self.framebuffer)
                .expect("Invalid framebuffer id in drawcall");
            if let DrawPass::Synced(id, trans) = &self.pass {
                bundle
                    .pass_container
                    .get_pass_concrete(
                        device,
                        *id,
                        attachments,
                        &bundle.texture_container,
                        &bundle.shader_container,
                        trans,
                    )
                    .expect("Couldn't create concrete pass")
            } else {
                log::warn!("Not synced draw call");
                return None;
            }
        };
        let framebuffer = bundle
            .framebuffer_container
            .get_concrete_framebuffer(device, &bundle.texture_container, self.framebuffer, &pass)
            .expect("Couldn't create concrete framebuffer");
        let pipeline = pass.pipeline();
        let _view_ids = framebuffer.views_id();
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
            if let Some(desc_writer) = &self.desc
                && let Ok(descriptor) = bundle.descriptor_container.get_descriptor_group(
                    desc_writer,
                    device,
                    &bundle.pass_container,
                    self.pass.pass_id(),
                    &mut bundle.texture_container,
                    &bundle.buffer_container,
                )
                && !descriptor.binded().is_empty()
            {
                device.cmd_bind_descriptor_sets(
                    command_buffer,
                    PipelineBindPoint::GRAPHICS,
                    pipeline.pipeline_layout().pipeline_layout(),
                    0,
                    descriptor.handles(),
                    &[],
                );
            }
            match self.geometry {
                DrawGeometry::Buffered { vbo, ebo } => {
                    let vertex_buffer = bundle.buffer_container.get_general_buffer(vbo)?;
                    let index_buffer =
                        ebo.and_then(|x| bundle.buffer_container.get_general_buffer(x));

                    let buffers = [vertex_buffer.handle()];
                    let offsets = [0];
                    device.cmd_bind_vertex_buffers(command_buffer, 0, &buffers, &offsets);
                    if let Some(_index_buffer) = index_buffer {
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

    pub fn framebuffer(&self) -> FramebufferId {
        self.framebuffer
    }
}

impl DrawData {}
