use std::{collections::HashMap, error::Error, thread::ThreadId};

use ash::vk::{
    DescriptorBufferInfo, DescriptorImageInfo, DescriptorPool, DescriptorPoolCreateFlags,
    DescriptorPoolCreateInfo, DescriptorPoolSize, DescriptorSetAllocateInfo, DescriptorSetLayout,
    DescriptorType, ImageLayout, PipelineStageFlags, WriteDescriptorSet,
};
use slotmap::SlotMap;

use crate::{
    device::DeviceContext,
    rendering::{
        buffer_container::{BufferContainer, GeneralBufferId, UniformBufferId, UniformData},
        shader_container::{DescriptorBinding, ShaderLayout},
        texture_container::{SamplingOptions, TextureContainer, TextureViewId},
    },
};

#[derive(Debug, Clone, Default)]
pub struct DescriptorContainer {
    allocators: HashMap<(ThreadId, Vec<DescriptorBinding>), Vec<(DescriptorPool, usize)>>,
    descriptors: SlotMap<RawDescriptorId, DescriptorSetGroup>,
}
impl DescriptorContainer {
    pub fn new() -> Self {
        Self::default()
    }
    fn gen_pool(
        &mut self,
        device: &DeviceContext,
        shader_layout: &ShaderLayout,
        capacity: u32,
    ) -> Result<DescriptorPool, Box<dyn Error>> {
        let desc_pool_sizes = shader_layout.descriptor_pool_sizes();
        let desc_pool_create = DescriptorPoolCreateInfo::default()
            .pool_sizes(&desc_pool_sizes)
            .max_sets(capacity);
        unsafe { Ok(device.create_descriptor_pool(&desc_pool_create, None)?) }
    }
    pub fn create_descriptor_set(
        &mut self,
        device: &DeviceContext,
        shader_layout: ShaderLayout,
    ) -> Result<DescriptorId, Box<dyn Error>> {
        let pool = if let Some(pool) = self
            .allocators
            .get_mut(&(std::thread::current().id(), shader_layout.bindings.clone()))
        {
            let pool = pool.last_mut().unwrap();
            if pool.1 < 1000 {
                pool
            } else {
                let new_gen = Self::gen_pool(self, device, &shader_layout, 1000)?;
                self.allocators.insert(
                    (std::thread::current().id(), shader_layout.bindings.clone()),
                    vec![(new_gen, 0)],
                );
                self.allocators
                    .get_mut(&(std::thread::current().id(), shader_layout.bindings.clone()))
                    .unwrap()
                    .last()
                    .unwrap()
            }
        } else {
            let new_gen = Self::gen_pool(self, device, &shader_layout, 1000)?;
            self.allocators.insert(
                (std::thread::current().id(), shader_layout.bindings.clone()),
                vec![(new_gen, 0)],
            );
            self.allocators
                .get_mut(&(std::thread::current().id(), shader_layout.bindings.clone()))
                .unwrap()
                .last()
                .unwrap()
        };
        let set_layouts = shader_layout
            .descriptor_layouts(device)
            .into_iter()
            .map(|x| x.1)
            .collect::<Vec<ash::vk::DescriptorSetLayout>>();
        let alloc = DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool.0)
            .set_layouts(&set_layouts);
        let desc_set = unsafe { device.allocate_descriptor_sets(&alloc) }?;
        let descriptor_sets = DescriptorSetGroup {
            handles: desc_set,
            layouts: set_layouts,
            shader_layout: shader_layout.clone(),
            binded: HashMap::new(),
        };
        let id = self.descriptors.insert(descriptor_sets);
        let desc_id = DescriptorId {
            id,
            name_to_binding: shader_layout.name_to_bind().clone(),
            set_buffers: HashMap::new(),
            set_samplers: HashMap::new(),
            set_textures: HashMap::new(),
        };
        Ok(desc_id)
    }
    pub fn apply_changes(
        &mut self,
        device: &DeviceContext,
        writer: &DescriptorId,
        buffer_container: &BufferContainer,
        texture_container: &mut TextureContainer,
    ) -> Option<()> {
        if let Some(sets) = self.descriptors.get_mut(writer.id()) {
            let buffers = writer
                .set_buffers()
                .iter()
                .filter_map(|x| {
                    if let Some(buff) = buffer_container.get_general_buffer(*x.1) {
                        Some((
                            [DescriptorBufferInfo::default()
                                .buffer(buff.handle())
                                .offset(0)
                                .range(buff.size())]
                            .to_vec(),
                            *x.0,
                        ))
                    } else {
                        log::warn!("failed writing buffer into descriptor set, buffer with id {:?} does not exists",x.0);
                        None
                    }
                })
                .collect::<Vec<(Vec<DescriptorBufferInfo>, DescriptorBinding)>>();
            let buf_writes = buffers
                .iter()
                .filter_map(|x| {
                    Some(
                        WriteDescriptorSet::default()
                            .buffer_info(&x.0)
                            .descriptor_count(1)
                            .dst_set(sets.handles()[x.1.set as usize])
                            .dst_binding(x.1.binding)
                            .descriptor_type(DescriptorType::UNIFORM_BUFFER),
                    )
                })
                .collect::<Vec<WriteDescriptorSet>>();

            let samplers = writer.set_samplers.iter().filter_map(|x| {
                if let Some(options) = texture_container.get_sampler(device, *x.1).map(|x|x.handle()) {
                    Some((
                        [DescriptorImageInfo::default().sampler(options)]
                        .to_vec(),
                        *x.0,
                    ))
                } else {
                    log::warn!("failed writing sampled texture into descriptor set, texture view with id {:?} does not exists",x.0);
                    None
                }
            }).collect::<Vec<(Vec<DescriptorImageInfo>,DescriptorBinding)>>();
            let textures = writer.set_textures.iter().filter_map(|x|
                if let Some(buff) = texture_container.get_image_view(*x.1) {
                    Some((
                        [DescriptorImageInfo::default().image_layout(ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(buff.handle())]
                        .to_vec(),
                        *x.0,
                    ))
                } else {
                    log::warn!("failed writing sampled texture into descriptor set, texture view with id {:?} does not exists",x.0);
                    None
                }
            ).collect::<Vec<(Vec<DescriptorImageInfo>,DescriptorBinding)>>();
            let sampler_writes = samplers
                .iter()
                .filter_map(|x| {
                    Some(
                        WriteDescriptorSet::default()
                            .image_info(&x.0)
                            .descriptor_count(1)
                            .dst_set(sets.handles()[x.1.set as usize])
                            .dst_binding(x.1.binding)
                            .descriptor_type(DescriptorType::SAMPLER),
                    )
                })
                .collect::<Vec<WriteDescriptorSet>>();
            let texture_writes = textures
                .iter()
                .filter_map(|x| {
                    Some(
                        WriteDescriptorSet::default()
                            .image_info(&x.0)
                            .descriptor_count(1)
                            .dst_set(sets.handles()[x.1.set as usize])
                            .dst_binding(x.1.binding)
                            .descriptor_type(DescriptorType::SAMPLED_IMAGE),
                    )
                })
                .collect::<Vec<WriteDescriptorSet>>();
            let writes = buf_writes
                .into_iter()
                .chain(sampler_writes)
                .chain(texture_writes)
                .collect::<Vec<WriteDescriptorSet>>();

            unsafe { device.update_descriptor_sets(&writes, &[]) };
            for buf in writer.set_buffers().iter() {
                sets.binded.insert(
                    *buf.0,
                    BindedRes::Buffer(*buf.1, 0, buf.1.len() * buf.1.item_size()),
                ); // todo: Add offset and size parameters
            }
            for sampler in writer.set_samplers.iter() {
                sets.binded
                    .insert(*sampler.0, BindedRes::Sampler(*sampler.1));
            }
            for tex in writer.set_textures.iter() {
                sets.binded.insert(*tex.0, BindedRes::Texture(*tex.1)); // todo: Add offset and size parameters
            }
            Some(())
        } else {
            None
        }
    }
    pub fn get_descriptor(&self, id: RawDescriptorId) -> Option<&DescriptorSetGroup> {
        self.descriptors.get(id)
    }
}
slotmap::new_key_type! {

    pub struct RawDescriptorId;
}

#[derive(Debug, Clone)]
pub struct DescriptorSetGroup {
    handles: Vec<ash::vk::DescriptorSet>,
    layouts: Vec<DescriptorSetLayout>,
    binded: HashMap<DescriptorBinding, BindedRes>,
    shader_layout: ShaderLayout,
}

impl DescriptorSetGroup {
    pub fn handles(&self) -> &Vec<ash::vk::DescriptorSet> {
        &self.handles
    }

    pub fn layouts(&self) -> &[DescriptorSetLayout] {
        &self.layouts
    }

    pub fn shader_layout(&self) -> &ShaderLayout {
        &self.shader_layout
    }

    pub fn binded(&self) -> &HashMap<DescriptorBinding, BindedRes> {
        &self.binded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorId {
    id: RawDescriptorId,
    name_to_binding: HashMap<String, DescriptorBinding>,
    set_buffers: HashMap<DescriptorBinding, GeneralBufferId>,
    set_samplers: HashMap<DescriptorBinding, SamplingOptions>,
    set_textures: HashMap<DescriptorBinding, TextureViewId>,
}
impl DescriptorId {
    pub fn set_uniform_buffer<U: UniformData>(
        &mut self,
        name: &str,
        buffer: UniformBufferId<U>,
    ) -> Option<()> {
        let binding = self.name_to_binding.get(name)?;
        if binding.ty != DescriptorType::UNIFORM_BUFFER
            && binding.ty != DescriptorType::UNIFORM_BUFFER_DYNAMIC
        {
            log::warn!(
                "tried to set uniform buffer to binding with type of {:?}",
                binding.ty
            );
            return None;
        }
        self.set_buffers.insert(*binding, *buffer);
        Some(())
    }
    pub fn set_sampler(&mut self, name: &str, sampling_options: SamplingOptions) -> Option<()> {
        let binding = self.name_to_binding.get(name)?;
        if binding.ty != DescriptorType::SAMPLER {
            log::warn!(
                "Tried to set sampler to binding with type of {:?}",
                binding.ty
            );
            return None;
        }
        self.set_samplers.insert(*binding, sampling_options);
        Some(())
    }
    pub fn set_texture(&mut self, name: &str, texture: TextureViewId) -> Option<()> {
        let binding = self.name_to_binding.get(name)?;
        if binding.ty != DescriptorType::SAMPLED_IMAGE {
            log::warn!(
                "Tried to set texture to binding with type of {:?}",
                binding.ty
            );
            return None;
        }
        self.set_textures.insert(*binding, texture);
        Some(())
    }
    pub fn id(&self) -> RawDescriptorId {
        self.id
    }

    pub fn set_buffers(&self) -> &HashMap<DescriptorBinding, GeneralBufferId> {
        &self.set_buffers
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindedRes {
    Buffer(GeneralBufferId, u64, u64),
    Sampler(SamplingOptions),
    Texture(TextureViewId),
}
