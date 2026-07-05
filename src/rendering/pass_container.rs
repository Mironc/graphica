use std::error::Error;

use ash::vk::{AccessFlags, ImageLayout, PipelineStageFlags};
use slotmap::SlotMap;

use crate::{
    device::DeviceContext,
    render_graph::{
        resource::ResourceId,
        resource_state::{ResourceAccess, ResourceState, ResourceUsage},
    },
    rendering::{
        buffer_container::VertexData,
        pipeline_container::{Pipeline, PipelineContainer, PipelineId, PipelineOptions},
        render_pass_container::{
            RenderPass, RenderPassAttachment, RenderPassContainer, RenderPassId, RenderPassSync,
        },
        shader_container::{Shader, ShaderContainer, ShaderId, ShaderLayout, ShaderType},
        texture_container::{Texture, TextureContainer, TextureViewId},
    },
};

use super::render_pass_container::{LoadOption, StoreOption};

#[derive(Default)]
pub struct PassContainer {
    pub render_pass_container: RenderPassContainer,
    pipeline_container: PipelineContainer,
    passes: SlotMap<PassId, PassLayout>,
}
impl PassContainer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_pass<V: VertexData>(
        &mut self,
        shader_container: &ShaderContainer,
        shader_ids: Vec<ShaderId>,
    ) -> Result<PassId, Box<dyn Error>> {
        let shaders = shader_ids
            .iter()
            .filter_map(|&x| shader_container.get(x))
            .collect::<Vec<&Shader>>();
        if shaders.is_empty() {
            return Err(<Box<dyn Error>>::from("No shaders was provided"));
        }
        let vertex_shader = shaders
            .iter()
            .find(|x| x.shader_type() == ShaderType::Vertex)
            .expect("No vertex shader provided");
        let fragment_shader = shaders
            .iter()
            .find(|x| x.shader_type() == ShaderType::Fragment)
            .expect("no fragment shader provided");
        let combined_layout = fragment_shader
            .shader_layout()
            .combine(vertex_shader.shader_layout())
            .ok_or_else(|| <Box<dyn Error>>::from("Incompatible shaders"))?;
        let render_pass_id = self
            .render_pass_container
            .render_pass_layout(combined_layout.output_types().clone());
        let pipeline_id = self
            .pipeline_container
            .create_pipeline_layout::<V>(shader_ids);
        Ok(self.passes.insert(PassLayout::new(
            combined_layout,
            render_pass_id,
            pipeline_id,
        )))
    }
    pub fn get_pass(&self, pass_id: PassId) -> Option<&PassLayout> {
        self.passes.get(pass_id)
    }
    pub fn get_pass_concrete(
        &mut self,
        device: &DeviceContext,
        pass_id: PassId,
        attachments: &[TextureViewId],
        texture_container: &TextureContainer,
        shader_container: &ShaderContainer,
        render_pass_transitions: &[(ResourceState, Option<ResourceState>)],
    ) -> Option<Pass> {
        let mut ps_in = PipelineStageFlags::empty();
        let mut ps_out = PipelineStageFlags::empty();
        let mut ac_in = AccessFlags::empty();
        let mut ac_out = AccessFlags::empty();

        let layout = self.get_pass(pass_id)?.clone();

        // Leave only attachment transitions
        let transitions = attachments
            .iter()
            .filter_map(|x| {
                render_pass_transitions.iter().find_map(|y| {
                    if y.0.resource_id() == ResourceId::Texture(x.texture()) {
                        let tex = texture_container.get_image(x.texture())?;
                        return Some((tex, y));
                    }
                    None
                })
            })
            .collect::<Vec<(&Texture, &(ResourceState, Option<ResourceState>))>>();

        let render_attachments = transitions
            .iter()
            .filter_map(|x| {
                let usage_to = x.1.1.map(|x| x.resource_usage()).unwrap_or_else(|| {
                    ResourceUsage::Texture(
                        ImageLayout::UNDEFINED,
                        PipelineStageFlags::TOP_OF_PIPE,
                        AccessFlags::empty(),
                        ResourceAccess::Read,
                    )
                });
                match (x.1.0.resource_usage(), usage_to) {
                    (
                        ResourceUsage::Texture(initial_layout, ps, ac, _)
                        | ResourceUsage::TextureTranstional(_, initial_layout, _, ps, _, ac, _),
                        ResourceUsage::Texture(final_layout, ps1, ac1, _)
                        | ResourceUsage::TextureTranstional(final_layout, _, ps1, _, ac1, _, _),
                    ) => {
                        ps_in |= ps;
                        ps_out |= ps1;
                        ac_in |= ac;
                        ac_out |= ac1;
                        Some(
                            RenderPassAttachment::new()
                                .format(x.0.texture_format())
                                .initial_layout(initial_layout)
                                .final_layout(final_layout)
                                .load_op(LoadOption::Load)
                                .store_op(StoreOption::Store)
                                .stencil_load_op(LoadOption::Load)
                                .stencil_store_op(StoreOption::Store),
                        )
                    }
                    _ => None,
                }
            })
            .collect::<Vec<RenderPassAttachment>>();

        let render_pass = self
            .render_pass_container
            .get_concrete_render_pass(
                device,
                layout.render_pass_id(),
                render_attachments,
                RenderPassSync::new(ps_in, ac_in, ps_out, ac_out),
            )
            .expect("Couldn't create render pass");

        let pipeline = self
            .pipeline_container
            .get_concrete_pipeline(
                device,
                shader_container,
                PipelineOptions {},
                render_pass.clone(),
                layout.pipeline_id,
            )
            .expect("Couldn't create graphics pipeline");

        Some(Pass {
            render_pass: render_pass.clone(),
            pipeline: pipeline.clone(),
        })
    }
}
slotmap::new_key_type! {
    pub struct PassId;
}
#[derive(Debug, Clone)]
pub struct PassLayout {
    shader_layout: ShaderLayout,
    render_pass_id: RenderPassId,
    pipeline_id: PipelineId,
}

impl PassLayout {
    pub fn new(
        shader_layout: ShaderLayout,
        render_pass_id: RenderPassId,
        pipeline_id: PipelineId,
    ) -> Self {
        Self {
            shader_layout,
            render_pass_id,
            pipeline_id,
        }
    }

    pub fn shader_layout(&self) -> &ShaderLayout {
        &self.shader_layout
    }

    pub fn render_pass_id(&self) -> RenderPassId {
        self.render_pass_id
    }

    pub fn pipeline_id(&self) -> PipelineId {
        self.pipeline_id
    }
}
#[derive(Debug)]
pub struct Pass {
    render_pass: RenderPass,
    pipeline: Pipeline,
}

impl Pass {
    pub fn render_pass(&self) -> &RenderPass {
        &self.render_pass
    }

    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }
}
