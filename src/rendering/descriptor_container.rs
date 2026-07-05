use std::{collections::HashMap, error::Error, thread::ThreadId};

use ash::vk::{
    self, DescriptorBufferInfo, DescriptorImageInfo, DescriptorPool, DescriptorPoolCreateInfo,
    DescriptorSetAllocateInfo, DescriptorSetLayout, DescriptorType, ImageLayout,
    WriteDescriptorSet,
};
use dashmap::{DashMap, mapref::one::MappedRefMut};

use crate::{
    device::DeviceContext,
    rendering::{
        buffer_container::{BufferContainer, GeneralBufferId, UniformBufferId, UniformData},
        pass_container::{PassContainer, PassId},
        shader_container::{DescriptorBinding, ShaderLayout},
        texture_container::{SamplingOptions, TextureContainer, TextureViewId},
    },
};
#[derive(Debug, Default)]
pub struct DescriptorContainer {
    pools: DashMap<(ThreadId, Vec<DescriptorBinding>), LocalDescriptors>,
}
impl DescriptorContainer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_descriptor_group<'a>(
        &'a self,
        writer: &DescriptorWriter,
        device: &DeviceContext,
        pass_container: &PassContainer,
        pass_id: PassId,
        texture_container: &mut TextureContainer,
        buffer_container: &BufferContainer,
    ) -> Result<
        MappedRefMut<'a, (ThreadId, Vec<DescriptorBinding>), LocalDescriptors, DescriptorSetGroup>,
        Box<dyn Error>,
    > {
        let layout = pass_container
            .get_pass(pass_id)
            .ok_or(<Box<dyn Error>>::from(format!(
                "No pass with id {:?}",
                pass_id
            )))?
            .shader_layout();
        let entry = self
            .pools
            .entry((std::thread::current().id(), layout.bindings.clone()))
            .or_insert_with(|| LocalDescriptors::create(device, layout));

        Ok(entry.map(|e| {
            e.get_descriptor_group(writer, device, layout, buffer_container, texture_container)
        }))
    }
    pub fn binded_res(
        &self,
        writer: &DescriptorWriter,
        pass_container: &PassContainer,
        pass_id: PassId,
    ) -> Result<Vec<(DescriptorBinding, BindedRes)>, Box<dyn Error>> {
        let shader_layout = pass_container
            .get_pass(pass_id)
            .ok_or(<Box<dyn Error>>::from(format!(
                "No pass with id {:?}",
                pass_id
            )))?
            .shader_layout();
        let mut res = Vec::with_capacity(
            writer.set_buffers().len() + writer.set_textures().len() + writer.set_samplers().len(),
        );
        for buf in writer.set_buffers().iter() {
            if let Some(binding) = shader_layout.name_to_bind().get(buf.0) {
                res.push((
                    *binding,
                    BindedRes::Buffer(*buf.1, 0, buf.1.len() * buf.1.item_size()),
                ));
            }
        }
        for samp in writer.set_samplers().iter() {
            if let Some(binding) = shader_layout.name_to_bind().get(samp.0) {
                res.push((*binding, BindedRes::Sampler(*samp.1)));
            }
        }
        for tex in writer.set_textures().iter() {
            if let Some(binding) = shader_layout.name_to_bind().get(tex.0) {
                res.push((*binding, BindedRes::Texture(*tex.1)));
            }
        }
        Ok(res)
    }
}
#[derive(Debug)]
pub struct LocalDescriptors {
    allocators: Vec<DescriptorAllocator>,
    descriptor_groups: Vec<(DescriptorWriter, DescriptorSetGroup)>,
}
impl LocalDescriptors {
    pub fn create(device: &DeviceContext, shader_layout: &ShaderLayout) -> Self {
        Self {
            allocators: vec![DescriptorAllocator::create(device, shader_layout)],
            descriptor_groups: Vec::new(),
        }
    }
    pub fn get_descriptor_group(
        &mut self,
        writer: &DescriptorWriter,
        device: &DeviceContext,
        shader_layout: &ShaderLayout,
        buffer_container: &BufferContainer,
        texture_container: &mut TextureContainer,
    ) -> &mut DescriptorSetGroup {
        if let Some(index) = self.descriptor_groups.iter().position(|(w, _)| w == writer) {
            return &mut self.descriptor_groups[index].1;
        }

        let pool = self.choose_pool(device, shader_layout);
        let mut group = DescriptorSetGroup::new(device, pool, shader_layout);

        group.update(
            device,
            shader_layout,
            writer,
            buffer_container,
            texture_container,
        );
        self.descriptor_groups.push((writer.clone(), group));
        let last = self.descriptor_groups.len() - 1;
        &mut self.descriptor_groups[last].1
    }
    fn choose_pool(
        &mut self,
        device: &DeviceContext,
        shader_layout: &ShaderLayout,
    ) -> &mut DescriptorAllocator {
        let last = self.allocators.last().unwrap();
        if last.exhausted() {
            self.allocators
                .push(DescriptorAllocator::create(device, shader_layout));
            self.allocators.last_mut().unwrap()
        } else {
            // this looks cursed ngl
            self.allocators.last_mut().unwrap()
        }
    }
}
const DESC_ALLOC_SIZE: usize = 1000;
#[derive(Debug)]
pub struct DescriptorAllocator {
    handle: DescriptorPool,
    alloc_count: usize,
}
impl DescriptorAllocator {
    pub(crate) fn create(device: &DeviceContext, shader_layout: &ShaderLayout) -> Self {
        let desc_pool_sizes = shader_layout.descriptor_pool_sizes();
        let desc_pool_create = DescriptorPoolCreateInfo::default()
            .pool_sizes(&desc_pool_sizes)
            .max_sets(DESC_ALLOC_SIZE as u32);
        let pool = unsafe { device.create_descriptor_pool(&desc_pool_create, None) }
            .expect("Couldn't create descriptor pool:");

        DescriptorAllocator {
            handle: pool,
            alloc_count: 0,
        }
    }

    /// When allocator is exhausted can not allocate new groups.
    pub fn exhausted(&self) -> bool {
        self.alloc_count >= DESC_ALLOC_SIZE
    }

    /// Allocates descriptors sets.
    ///
    /// Returns `None` if allocator is exhausted.
    pub fn allocate_set(
        &mut self,
        device: &DeviceContext,
        set_layouts: &[DescriptorSetLayout],
    ) -> Option<Vec<vk::DescriptorSet>> {
        let allocate_info = DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.handle)
            .set_layouts(set_layouts);
        if !self.exhausted() {
            Some(
                unsafe { device.allocate_descriptor_sets(&allocate_info) }
                    .expect("Couldn't allocate descriptor sets:"),
            )
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct DescriptorSetGroup {
    handles: Vec<ash::vk::DescriptorSet>,
    layouts: Vec<DescriptorSetLayout>,
    binded: HashMap<DescriptorBinding, BindedRes>,
    shader_layout: ShaderLayout,
}
impl DescriptorSetGroup {
    pub(crate) fn new(
        device: &DeviceContext,
        pool: &mut DescriptorAllocator,
        shader_layout: &ShaderLayout,
    ) -> Self {
        let set_layouts = shader_layout
            .descriptor_layouts(device)
            .into_iter()
            .map(|x| x.1)
            .collect::<Vec<ash::vk::DescriptorSetLayout>>();
        let desc_set = pool
            .allocate_set(device, &set_layouts)
            .expect("Tried to allocate for an exhausted allocator");
        DescriptorSetGroup {
            handles: desc_set,
            layouts: set_layouts,
            shader_layout: shader_layout.clone(),
            binded: HashMap::new(),
        }
    }

    pub(crate) fn update(
        &mut self,
        device: &DeviceContext,
        shader_layout: &ShaderLayout,
        writer: &DescriptorWriter,
        buffer_container: &BufferContainer,
        texture_container: &mut TextureContainer,
    ) -> Option<()> {
        let buffers = writer
                .set_buffers()
                .iter()
                .filter_map(|x| {
                    if let Some(buff) = buffer_container.get_general_buffer(*x.1) && let Some(binding) = shader_layout.name_to_bind().get(x.0) {
                        Some((
                            [DescriptorBufferInfo::default()
                                .buffer(buff.handle())
                                .offset(0)
                                .range(buff.size())]
                            .to_vec(),
                            *binding,
                        ))
                    } else {
                        if shader_layout.name_to_bind().get(x.0).is_some(){
                            log::warn!("failed writing buffer into descriptor set, buffer with id {:?} does not exists",x.0);
                        }
                        else{
                            log::warn!("provided non-existent field {}",x.0)
                        }
                        None
                    }
                })
                .collect::<Vec<(Vec<DescriptorBufferInfo>, DescriptorBinding)>>();
        let buf_writes = buffers
            .iter()
            .map(|x| {
                WriteDescriptorSet::default()
                    .buffer_info(&x.0)
                    .descriptor_count(1)
                    .dst_set(self.handles()[x.1.set as usize])
                    .dst_binding(x.1.binding)
                    .descriptor_type(DescriptorType::UNIFORM_BUFFER)
            })
            .collect::<Vec<WriteDescriptorSet>>();

        let samplers = writer.set_samplers.iter().filter_map(|x| {
            if let Some(options) = texture_container.get_sampler(device, *x.1).map(|x|x.handle()) && let Some(binding) = shader_layout.name_to_bind().get(x.0) {
                    Some((
                        [DescriptorImageInfo::default().sampler(options)]
                        .to_vec(),
                        *binding,
                    ))
                } else {
                    if shader_layout.name_to_bind().get(x.0).is_some(){
                        log::warn!("failed writing sampler into descriptor set, sampler with id {:?} does not exists",x.0);
                    }
                    else{
                        log::warn!("provided non-existent field {}",x.0)
                    }
                    None
                }
            }).collect::<Vec<(Vec<DescriptorImageInfo>,DescriptorBinding)>>();
        let textures = writer.set_textures.iter().filter_map(|x|
                if let Some(buff) = texture_container.get_image_view(*x.1) && let Some(binding) = shader_layout.name_to_bind().get(x.0) {
                    Some((
                        [DescriptorImageInfo::default().image_layout(ImageLayout::SHADER_READ_ONLY_OPTIMAL).image_view(buff.handle())]
                        .to_vec(),
                        *binding,
                    ))
                } else {
                    if shader_layout.name_to_bind().get(x.0).is_some(){
                        log::warn!("failed writing sampled texture into descriptor set, texture view with id {:?} does not exists",x.0);
                    }
                    else{
                        log::warn!("provided non-existent field {}",x.0)
                    }
                    None
                }
            ).collect::<Vec<(Vec<DescriptorImageInfo>,DescriptorBinding)>>();
        let sampler_writes = samplers
            .iter()
            .map(|x| {
                WriteDescriptorSet::default()
                    .image_info(&x.0)
                    .descriptor_count(1)
                    .dst_set(self.handles()[x.1.set as usize])
                    .dst_binding(x.1.binding)
                    .descriptor_type(DescriptorType::SAMPLER)
            })
            .collect::<Vec<WriteDescriptorSet>>();
        let texture_writes = textures
            .iter()
            .map(|x| {
                WriteDescriptorSet::default()
                    .image_info(&x.0)
                    .descriptor_count(1)
                    .dst_set(self.handles()[x.1.set as usize])
                    .dst_binding(x.1.binding)
                    .descriptor_type(DescriptorType::SAMPLED_IMAGE)
            })
            .collect::<Vec<WriteDescriptorSet>>();
        let writes = buf_writes
            .into_iter()
            .chain(sampler_writes)
            .chain(texture_writes)
            .collect::<Vec<WriteDescriptorSet>>();

        unsafe { device.update_descriptor_sets(&writes, &[]) };
        for buf in writer.set_buffers().iter() {
            if let Some(binding) = shader_layout.name_to_bind().get(buf.0) {
                self.binded.insert(
                    *binding,
                    BindedRes::Buffer(*buf.1, 0, buf.1.len() * buf.1.item_size()),
                ); // todo: Add offset and size parameters
            }
        }
        for sampler in writer.set_samplers.iter() {
            if let Some(binding) = shader_layout.name_to_bind().get(sampler.0) {
                self.binded.insert(*binding, BindedRes::Sampler(*sampler.1));
            }
        }
        for tex in writer.set_textures.iter() {
            if let Some(binding) = shader_layout.name_to_bind().get(tex.0) {
                self.binded.insert(*binding, BindedRes::Texture(*tex.1));
            }
        }
        Some(())
    }
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescriptorWriter {
    set_buffers: HashMap<String, GeneralBufferId>,
    set_samplers: HashMap<String, SamplingOptions>,
    set_textures: HashMap<String, TextureViewId>,
}
impl DescriptorWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_uniform_buffer<U: UniformData>(&mut self, name: String, buffer: UniformBufferId<U>) {
        self.set_buffers.insert(name, *buffer);
    }
    pub fn set_sampler(&mut self, name: String, sampling_options: SamplingOptions) {
        self.set_samplers.insert(name, sampling_options);
    }
    pub fn set_texture(&mut self, name: String, texture: TextureViewId) {
        self.set_textures.insert(name, texture);
    }

    pub fn set_buffers(&self) -> &HashMap<String, GeneralBufferId> {
        &self.set_buffers
    }

    pub fn set_samplers(&self) -> &HashMap<String, SamplingOptions> {
        &self.set_samplers
    }

    pub fn set_textures(&self) -> &HashMap<String, TextureViewId> {
        &self.set_textures
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindedRes {
    Buffer(GeneralBufferId, u64, u64),
    Sampler(SamplingOptions),
    Texture(TextureViewId),
}
