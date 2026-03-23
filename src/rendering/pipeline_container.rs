use std::collections::HashMap;
use std::error::Error;
use std::ffi::CString;
use std::marker::PhantomData;

use ash::vk::{
    BlendFactor, BlendOp, ColorComponentFlags, CompareOp, CullModeFlags, DescriptorSetLayout,
    DynamicState, FrontFace, GraphicsPipelineCreateInfo, LogicOp, PipelineCache,
    PipelineColorBlendAttachmentState, PipelineColorBlendStateCreateInfo,
    PipelineDepthStencilStateCreateInfo, PipelineDynamicStateCreateInfo,
    PipelineInputAssemblyStateCreateInfo, PipelineLayoutCreateInfo,
    PipelineMultisampleStateCreateInfo, PipelineRasterizationStateCreateInfo,
    PipelineShaderStageCreateInfo, PipelineVertexInputStateCreateInfo,
    PipelineViewportStateCreateInfo, PolygonMode, PrimitiveTopology, PushConstantRange,
    SampleCountFlags, ShaderModuleCreateInfo, ShaderStageFlags, VertexInputAttributeDescription,
    VertexInputBindingDescription, VertexInputRate,
};
use slotmap::SlotMap;

use crate::device::DeviceContext;
use crate::rendering::buffer_container::VertexData;
use crate::rendering::render_pass_container::RenderPass;
use crate::rendering::shader_container::{
    Shader, ShaderContainer, ShaderId, ShaderLayout, ShaderType,
};

#[derive(Default)]
pub struct PipelineContainer {
    pipelines: SlotMap<PipelineId, Pipeline>,
}
impl PipelineContainer {
    pub fn new() -> Self {
        Self {
            pipelines: SlotMap::default(),
        }
    }
    pub fn create_pipeline<V: VertexData>(
        &mut self,
        device: &DeviceContext,
        shader_container: &ShaderContainer,
        create: CreatePipeline<V>,
    ) -> Result<PipelineId, Box<dyn Error>> {
        let shaders = create
            .shaders
            .iter()
            .map(|&x| shader_container.get(x))
            .filter_map(|x| {
                if let Some(shader) = x {
                    return Some(shader);
                } else {
                    None
                }
            })
            .collect::<Vec<&Shader>>();
        if shaders.len() == 0 {
            return Err(<Box<dyn Error>>::from("No shaders was provided"));
        }
        let vertex_shader = shaders
            .iter()
            .find(|x| x.shader_type() == ShaderType::Vertex)
            .unwrap();
        let fragment_shader = shaders
            .iter()
            .find(|x| x.shader_type() == ShaderType::Fragment)
            .unwrap();
        let combined_layout = fragment_shader
            .shader_layout()
            .combine(vertex_shader.shader_layout())
            .ok_or_else(|| <Box<dyn Error>>::from("Incompatible shaders"))?;

        let shadermodule_createinfo =
            ShaderModuleCreateInfo::default().code(vertex_shader.source());
        let vertex_module = unsafe { device.create_shader_module(&shadermodule_createinfo, None) }?;

        let shadermodule_createinfo =
            ShaderModuleCreateInfo::default().code(fragment_shader.source());
        let fragment_module =
            unsafe { device.create_shader_module(&shadermodule_createinfo, None) }?;

        let entry_point_name = CString::new("main").unwrap();
        let vertex_shader_state_info = PipelineShaderStageCreateInfo::default()
            .stage(ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(&entry_point_name);
        let fragment_shader_state_info = PipelineShaderStageCreateInfo::default()
            .stage(ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(&entry_point_name);
        let shader_states_infos = [vertex_shader_state_info, fragment_shader_state_info];
        let vertex_layout = V::layout_info();

        let vertex_attribute_descs = vertex_layout
            .into_iter()
            .fold(Vec::new(), |mut acc, mut x| {
                acc.append(&mut x);
                acc
            })
            .into_iter()
            .enumerate()
            .map(|(binding, x)| {
                VertexInputAttributeDescription::default()
                    .binding(0)
                    .location(x.location() as u32)
                    .format(x.format().into_format())
                    .offset(x.offset() as u32)
            })
            .collect::<Vec<VertexInputAttributeDescription>>();
        let vertex_binding_descs = if !vertex_attribute_descs.is_empty() {
            [VertexInputBindingDescription::default()
                .binding(0)
                .stride(size_of::<V>() as _)
                .input_rate(VertexInputRate::VERTEX)]
            .to_vec()
        } else {
            [].to_vec()
        };
        let vertex_input_info = PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&vertex_binding_descs)
            .vertex_attribute_descriptions(&vertex_attribute_descs);

        // TODO:Make those not hardcoded
        let input_assembly_info = PipelineInputAssemblyStateCreateInfo::default()
            .topology(PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);
        let rasterizer_info = PipelineRasterizationStateCreateInfo::default()
            .depth_clamp_enable(false)
            .rasterizer_discard_enable(false)
            .polygon_mode(PolygonMode::FILL)
            .line_width(1.0)
            .cull_mode(CullModeFlags::BACK)
            .front_face(FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(false)
            .depth_bias_constant_factor(0.0)
            .depth_bias_clamp(0.0)
            .depth_bias_slope_factor(0.0);

        let depth_stencil_info = PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(1.0)
            .stencil_test_enable(false)
            .front(Default::default())
            .back(Default::default());

        let color_blend_attachment = PipelineColorBlendAttachmentState::default()
            .color_write_mask(ColorComponentFlags::RGBA)
            .blend_enable(false)
            .src_color_blend_factor(BlendFactor::ONE)
            .dst_color_blend_factor(BlendFactor::ZERO)
            .color_blend_op(BlendOp::ADD)
            .src_alpha_blend_factor(BlendFactor::ONE)
            .dst_alpha_blend_factor(BlendFactor::ZERO)
            .alpha_blend_op(BlendOp::ADD);
        let color_blend_attachments = [color_blend_attachment];

        let color_blending_info = PipelineColorBlendStateCreateInfo::default()
            .logic_op_enable(false)
            .logic_op(LogicOp::COPY)
            .attachments(&color_blend_attachments)
            .blend_constants([0.0, 0.0, 0.0, 0.0]);

        let layout = {
            let mut desc_layouts = combined_layout.descriptor_layouts(device);
            desc_layouts.sort_by(|x, x1| x.0.cmp(&x1.0));
            let layouts = desc_layouts
                .iter()
                .map(|x| x.1)
                .collect::<Vec<DescriptorSetLayout>>();
            let mut stages = ash::vk::ShaderStageFlags::empty();
            for s in combined_layout.push_constants.iter() {
                stages |= s.stage_flags;
            }
            let ranges = if combined_layout.push_constants.is_empty() {
                [].to_vec()
            } else {
                [PushConstantRange::default()
                    .offset(0)
                    .size(128)
                    .stage_flags(stages)]
                .to_vec()
            };
            let layout_info = PipelineLayoutCreateInfo::default()
                .set_layouts(&layouts)
                .push_constant_ranges(&ranges);

            unsafe { device.create_pipeline_layout(&layout_info, None).unwrap() }
        };
        let pipeline_layout = PipelineLayout {
            pipeline_layout: layout,
            shader_layout: combined_layout,
        };

        let render_pass = create
            .render_pass
            .ok_or_else(|| <Box<dyn Error>>::from("No renderpass is set"))?;

        let dynamic_states = [DynamicState::VIEWPORT, DynamicState::SCISSOR];
        let dynamic_states_createinfo =
            PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let multisample_createinfo = PipelineMultisampleStateCreateInfo::default()
            //.min_sample_shading(1.0)
            .rasterization_samples(SampleCountFlags::TYPE_1);

        let viewport_state = PipelineViewportStateCreateInfo::default()
            .scissor_count(1)
            .viewport_count(1);
        let pipeline_info = GraphicsPipelineCreateInfo::default()
            .stages(&shader_states_infos)
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly_info)
            .dynamic_state(&dynamic_states_createinfo)
            .rasterization_state(&rasterizer_info)
            .multisample_state(&multisample_createinfo)
            .depth_stencil_state(&depth_stencil_info)
            .color_blend_state(&color_blending_info)
            .viewport_state(&viewport_state)
            .layout(layout)
            .render_pass(render_pass.handle())
            .subpass(0);
        // .base_pipeline_handle() null since it is not derived from another
        // .base_pipeline_index(-1) same
        let pipeline_infos = [pipeline_info];

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(PipelineCache::null(), &pipeline_infos, None)
                .unwrap()[0]
        };

        unsafe {
            device.destroy_shader_module(vertex_module, None);
            device.destroy_shader_module(fragment_module, None);
        };
        let pipeline = Pipeline {
            handle: pipeline,
            pipeline_layout,
            render_pass: render_pass.clone(),
        };
        Ok(self.pipelines.insert(pipeline))
    }
    pub fn get(&self, pipeline_id: PipelineId) -> Option<&Pipeline> {
        self.pipelines.get(pipeline_id)
    }
}
slotmap::new_key_type! {pub struct PipelineId;}
#[derive(Debug, Clone)]
pub struct Pipeline {
    handle: ash::vk::Pipeline,
    pipeline_layout: PipelineLayout,
    render_pass: RenderPass,
}

impl Pipeline {
    pub fn handle(&self) -> ash::vk::Pipeline {
        self.handle
    }

    pub fn render_pass(&self) -> &RenderPass {
        &self.render_pass
    }

    pub fn pipeline_layout(&self) -> &PipelineLayout {
        &self.pipeline_layout
    }
}
#[derive(Debug, Clone)]
pub struct CreatePipeline<'a, V: VertexData> {
    render_pass: Option<&'a RenderPass>,
    shaders: &'a [ShaderId],
    _marker: PhantomData<V>,
}
impl<'a, V> CreatePipeline<'a, V>
where
    V: VertexData,
{
    pub fn new() -> CreatePipeline<'a, V> {
        CreatePipeline::default()
    }
    pub fn render_pass(mut self, render_pass: &'a RenderPass) -> Self {
        self.render_pass = Some(render_pass);
        self
    }
    pub fn shaders(mut self, shaders: &'a [ShaderId]) -> Self {
        self.shaders = shaders;
        self
    }
}

impl<'a, V> Default for CreatePipeline<'a, V>
where
    V: VertexData,
{
    fn default() -> Self {
        Self {
            render_pass: Default::default(),
            shaders: Default::default(),
            _marker: Default::default(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct PipelineLayout {
    pipeline_layout: ash::vk::PipelineLayout,
    shader_layout: ShaderLayout,
}

impl PipelineLayout {
    pub fn pipeline_layout(&self) -> ash::vk::PipelineLayout {
        self.pipeline_layout
    }

    pub fn shader_layout(&self) -> &ShaderLayout {
        &self.shader_layout
    }
}
