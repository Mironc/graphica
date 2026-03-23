use std::collections::HashMap;
use std::error::Error;

use ash::vk::{self, DescriptorPoolSize, DescriptorType, PushConstantRange};
use encase::internal::BufferMut;
use naga::AddressSpace;
use naga::back::spv::PipelineOptions;
use slotmap::SlotMap;

#[derive(Debug, Default)]
pub struct ShaderContainer {
    shaders: SlotMap<ShaderId, Shader>,
}
impl ShaderContainer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        shader_source: &str,
        shader_type: ShaderType,
    ) -> Result<ShaderId, Box<dyn Error>> {
        // reading source glsl
        let mut parser = naga::front::glsl::Frontend::default();
        let options = naga::front::glsl::Options::from(shader_type.into_stage());
        let module = parser
            .parse(&options, &shader_source)
            .map_err(|e| eprintln!("{}", e.emit_to_string(shader_source)))
            .unwrap();

        let options = naga::back::spv::Options::default();
        let module_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .subgroup_stages(naga::valid::ShaderStages::all())
        .subgroup_operations(naga::valid::SubgroupOperationSet::all())
        .validate(&module)
        .map_err(|x| eprintln!("{}", x.emit_to_string(shader_source)))
        .unwrap();

        // Reflection
        let mut shader_layout = ShaderLayout {
            bindings: Vec::new(),
            name_to_bind: HashMap::new(),
            push_constants: Vec::new(),
            name_to_push: HashMap::new(),
        };
        for (handle, variable) in module.global_variables.iter() {
            if let Some(binding) = &variable.binding {
                let naga_ty = module.types[variable.ty].clone();
                let ty = match variable.space {
                    AddressSpace::Uniform => Some(vk::DescriptorType::UNIFORM_BUFFER),
                    AddressSpace::Storage { access } => Some(vk::DescriptorType::STORAGE_BUFFER),
                    AddressSpace::Handle => Some({
                        match module.types[variable.ty].inner {
                            naga::TypeInner::Image {
                                dim,
                                arrayed,
                                class,
                            } => match class {
                                naga::ImageClass::Sampled { kind, multi } => {
                                    vk::DescriptorType::SAMPLED_IMAGE
                                }
                                naga::ImageClass::Depth { multi } => {
                                    vk::DescriptorType::SAMPLED_IMAGE
                                }
                                naga::ImageClass::Storage { format, access } => {
                                    vk::DescriptorType::STORAGE_IMAGE
                                }
                            },
                            naga::TypeInner::Sampler { comparison } => vk::DescriptorType::SAMPLER,
                            naga::TypeInner::BindingArray { base, size } => todo!(),
                            _ => unreachable!(), //in theory
                        }
                    }),
                    _ => None,
                };
                if let (Some(ty), Some(name)) = (ty, variable.name.clone()) {
                    shader_layout.add_descriptor(
                        DescriptorBinding {
                            set: binding.group,
                            binding: binding.binding,
                            ty,
                            stage_flags: shader_type.into_stage_flag(),
                        },
                        name,
                    );
                }
            }
            if let AddressSpace::PushConstant = &variable.space {
                let naga_ty = module.types[variable.ty].clone();
                println!("Found push_constant {:?}", &naga_ty);
                let mut fields = Vec::new();
                match &naga_ty.inner {
                    naga::TypeInner::Struct { members, span } => {
                        for field in members.iter() {
                            println!("encountered field:{:?}", module.types[field.ty]);
                            if let Some(name) = &field.name {
                                let size = module.types[field.ty].inner.size(module.to_ctx());
                                fields.push((name.clone(), field.offset, size));
                            }
                        }
                    }
                    _ => unreachable!(), // don't come as any other type
                }
                for (field_name, offset, size) in fields.into_iter() {
                    if offset + size > 256 {
                        log::error!(
                            "Push constant size is greater than max size of 256 bytes

                            A piece of advice: keep larger objects such as matrixes and vectors at the first spots to reduce total size");
                        break;
                    }
                    if offset + size > 128 {
                        log::warn!(
                            "Push constant size is more than 128 bytes, that may cause compatibility problems on some platforms

                            A piece of advice: keep larger objects such as matrixes and vectors at the first spots to reduce total size"
                        );
                    }
                    let push_range = PushConstantRange::default()
                        .offset(offset)
                        .size(size)
                        .stage_flags(shader_type.into_stage_flag());
                    shader_layout.add_push(push_range, field_name);
                }
            }
        }
        println!("{:?}", shader_layout);

        //writing as spir-v
        let mut source_spv = vec![];
        naga::back::spv::Writer::new(&options)?
            .write(
                &module,
                &module_info,
                Some(&PipelineOptions {
                    shader_stage: shader_type.into_stage(),
                    entry_point: "main".to_owned(),
                }),
                &None,
                &mut source_spv,
            )
            .unwrap();
        let shader = Shader {
            source: source_spv,
            shader_type,
            layout: shader_layout,
        };

        Ok(self.shaders.insert(shader))
    }
    pub fn get(&self, id: ShaderId) -> Option<&Shader> {
        self.shaders.get(id)
    }
}
slotmap::new_key_type! {pub struct ShaderId;}
#[derive(Debug, Clone)]
pub struct Shader {
    source: Vec<u32>,
    shader_type: ShaderType,
    layout: ShaderLayout,
}

impl Shader {
    pub fn shader_type(&self) -> ShaderType {
        self.shader_type
    }

    pub fn shader_layout(&self) -> &ShaderLayout {
        &self.layout
    }

    pub fn source(&self) -> &[u32] {
        &self.source
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Vertex,
    Fragment,
    Compute,
}
impl ShaderType {
    pub fn into_stage(&self) -> naga::ShaderStage {
        match self {
            ShaderType::Vertex => naga::ShaderStage::Vertex,
            ShaderType::Fragment => naga::ShaderStage::Fragment,
            ShaderType::Compute => naga::ShaderStage::Compute,
        }
    }
    pub fn into_stage_flag(&self) -> vk::ShaderStageFlags {
        match self {
            ShaderType::Vertex => vk::ShaderStageFlags::VERTEX,
            ShaderType::Fragment => vk::ShaderStageFlags::FRAGMENT,
            ShaderType::Compute => vk::ShaderStageFlags::COMPUTE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorBinding {
    pub set: u32,
    pub binding: u32,
    pub ty: vk::DescriptorType,
    pub stage_flags: vk::ShaderStageFlags,
}
#[derive(Debug, Clone, Copy)]
pub enum DescriptorBindingType {
    Uniform,
    Storage,
}
#[derive(Debug, Default, Clone)]
pub struct ShaderLayout {
    pub bindings: Vec<DescriptorBinding>,
    name_to_bind: HashMap<String, DescriptorBinding>,
    pub push_constants: Vec<PushConstantRange>,
    name_to_push: HashMap<String, usize>,
}
impl ShaderLayout {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            name_to_bind: HashMap::new(),
            push_constants: Vec::new(),
            name_to_push: HashMap::new(),
        }
    }
    pub fn descriptor_layouts(&self, device: &ash::Device) -> Vec<(u32, vk::DescriptorSetLayout)> {
        let mut layouts = Vec::new();
        let mut set_bindings: HashMap<u32, Vec<vk::DescriptorSetLayoutBinding>> = HashMap::new();

        for info in &self.bindings {
            set_bindings.entry(info.set).or_default().push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(info.binding)
                    .descriptor_type(info.ty)
                    .descriptor_count(1)
                    .stage_flags(info.stage_flags),
            );
        }

        for (set, bindings) in set_bindings {
            let create_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            let layout = unsafe {
                device
                    .create_descriptor_set_layout(&create_info, None)
                    .unwrap()
            };
            layouts.push((set, layout));
        }
        layouts
    }
    pub fn descriptor_pool_sizes(&self) -> Vec<DescriptorPoolSize> {
        let mut by_type = HashMap::new();
        for desc_bind in self.bindings.iter() {
            let mut entry: &mut Vec<DescriptorBinding> = by_type.entry(desc_bind.ty).or_default();
            entry.push(*desc_bind);
        }
        let mut sizes = Vec::new();
        for v in by_type.into_iter() {
            let desc_size = DescriptorPoolSize::default()
                .ty(v.0)
                .descriptor_count(v.1.len() as u32);
            sizes.push(desc_size);
        }

        sizes
    }
    /// Combines `ShaderLayout` for two shaders
    pub fn combine(&self, other: &Self) -> Option<Self> {
        let mut bindings = self.bindings.clone();
        let mut push_constants = self.push_constants.clone();
        let mut name_bind = self.name_to_bind.clone();
        let mut name_push = self.name_to_push.clone();

        for other_binding in other.bindings.iter() {
            if let Some(binding) = bindings.get_mut(other_binding.binding as usize) {
                if binding.ty != other_binding.ty {
                    return None;
                }
                binding.stage_flags |= other_binding.stage_flags;
            }
        }
        {
            let mut offset = 0;
            for &push_constant in other.push_constants.iter() {
                if offset + push_constant.size > 256 {
                    log::error!(
                        "Push constant size from two shaders is greater than max size of 256 bytes

                        A piece of advice: keep larger objects such as matrixes and vectors at the first spots to reduce total size");
                    break;
                }
                if offset + push_constant.size > 128 {
                    log::warn!(
                        "Push constant size for two shaders is more than 128 bytes, that may cause compatibility problems on some platforms

                        A piece of advice: keep larger objects such as matrixes and vectors at the first spots to reduce total size"
                    );
                }
                let push_constant = push_constant.offset(offset);
                push_constants.push(push_constant);
                offset += push_constant.size;
            }
        }
        for a in other.name_to_bind.iter() {
            name_bind.insert(a.0.clone(), *a.1);
        }
        for a in other.name_to_push.iter() {
            name_push.insert(a.0.clone(), *a.1);
        }

        Some(Self {
            bindings,
            name_to_bind: name_bind,
            push_constants,
            name_to_push: name_push,
        })
    }
    pub fn add_descriptor(&mut self, desc_bind: DescriptorBinding, name: String) {
        self.bindings.push(desc_bind);
        self.name_to_bind.insert(name, desc_bind);
    }
    pub fn add_push(&mut self, push_range: PushConstantRange, name: String) {
        self.push_constants.push(push_range);
        self.name_to_push
            .insert(name, self.push_constants.len() - 1);
    }

    pub fn name_to_bind(&self) -> &HashMap<String, DescriptorBinding> {
        &self.name_to_bind
    }

    pub fn name_to_push(&self) -> &HashMap<String, usize> {
        &self.name_to_push
    }
    pub fn get_push_constant_writer(&self) -> PushWriter {
        PushWriter {
            push_constants: self.push_constants.clone(),
            name_to_push: self.name_to_push.clone(),
            buf: [0; 128],
        }
    }
}
#[derive(Debug, Clone)]
pub struct PushWriter {
    pub push_constants: Vec<PushConstantRange>,
    name_to_push: HashMap<String, usize>,
    /// Apparently, that 128 bytes is max guaranteed size for push_constants
    buf: [u8; 128],
}
impl PushWriter {
    /// Writes into push_constants buffer
    ///
    /// **Returns None if failed**
    fn write(&mut self, push_constant: PushConstantRange, data: &[u8]) -> Option<()> {
        if data.len() as u32 > push_constant.size {
            log::warn!(
                "Tried to write into PushConstant with data of size {} but expected {}",
                data.len(),
                push_constant.size
            );
            return None;
        }
        self.buf.write_slice(push_constant.offset as usize, data);
        Some(())
    }
    pub fn get_by_name(&self, name: &str) -> Option<PushConstantRange> {
        self.push_constants
            .get(*self.name_to_push.get(name)?)
            .copied()
    }
    pub fn vec4(&mut self, name: &str, vec4: [f32; 4]) -> Option<()> {
        let data = vec4
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<u8>>();

        self.write(self.get_by_name(name)?, &data)
    }
    pub fn vec3(&mut self, name: &str, vec3: [f32; 3]) -> Option<()> {
        let data = vec3
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<u8>>();

        self.write(self.get_by_name(name)?, &data)
    }
    pub fn vec2(&mut self, name: &str, vec2: [f32; 2]) -> Option<()> {
        let data = vec2
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<u8>>();

        self.write(self.get_by_name(name)?, &data)
    }
    pub fn f32(&mut self, name: &str, f32: f32) -> Option<()> {
        let data = f32.to_le_bytes();
        self.write(self.get_by_name(name)?, &data)
    }
    pub fn u32(&mut self, name: &str, u32: u32) -> Option<()> {
        let data = u32.to_le_bytes();
        self.write(self.get_by_name(name)?, &data)
    }

    pub fn buf(&self) -> [u8; 128] {
        self.buf
    }
}
#[test]
pub fn test_shader() {
    // glsl source is taken from https://github.com/stripe2933/vk-deferred
    let source = "
    #version 450
    #extension GL_ARB_separate_shader_objects : enable

    layout(location = 0) out vec3 fragColor;

    vec3 positions[3] = vec3[](
        vec3( 1.0,  -1.0, 0.0),
        vec3( 0.0, 1.0, 0.0),
        vec3(-1.0,  -1.0, 0.0)
    );

    vec3 colors[3] = vec3[](
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        vec3(0.0, 0.0, 1.0)
    );


    layout(push_constant) uniform PushConstants {
        float time;
    };
    void main() {
        gl_Position = vec4(positions[gl_VertexIndex], 1.0);
        fragColor = colors[gl_VertexIndex]*abs(sin(time));
    }";
    let mut shader_container = ShaderContainer::new();
    shader_container.insert(source, ShaderType::Vertex).unwrap();
}
