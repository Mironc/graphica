use std::{
    any::{Any, TypeId},
    error::Error,
    marker::PhantomData,
    ops::Deref,
};

use ash::vk::{BufferCreateInfo, BufferUsageFlags, Format, SharingMode};
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
};
use slotmap::SlotMap;

use crate::device::DeviceContext;

// pub uses
pub use encase::*;
pub use graphica_macro::VertexData;
pub use graphica_macro::{ShaderType, storage_data, uniform_data};

/// Centralized container for storing all that related to buffers
#[derive(Default)]
pub struct BufferContainer {
    buffers: SlotMap<BufferId, GeneralBuffer>,
}
impl BufferContainer {
    pub fn new() -> Self {
        Self {
            buffers: SlotMap::default(),
        }
    }
    /// Creates general (untyped) buffer
    ///
    /// # Possible error types:
    /// - AllocationError
    /// - VKError
    pub fn create_general_buffer<T: GpuData>(
        &mut self,
        device: &DeviceContext,
        usage: BufferUsage,
        type_size: u64,
        create_buffer: CreateBuffer<T>,
    ) -> Result<GeneralBufferId, Box<dyn Error>> {
        let usage = match usage {
            BufferUsage::Storage => BufferUsageFlags::STORAGE_BUFFER,
            BufferUsage::Uniform => BufferUsageFlags::UNIFORM_BUFFER,
            BufferUsage::Vertex => BufferUsageFlags::VERTEX_BUFFER,
            BufferUsage::Index => BufferUsageFlags::INDEX_BUFFER,
            BufferUsage::Staging => BufferUsageFlags::empty(),
        } | BufferUsageFlags::TRANSFER_DST
            | BufferUsageFlags::TRANSFER_SRC;

        let buffer_createinfo = BufferCreateInfo::default()
            .size(create_buffer.len * type_size)
            .usage(usage)
            .sharing_mode(SharingMode::EXCLUSIVE);
        let raw_buffer = unsafe { device.create_buffer(&buffer_createinfo, None) }?;
        let memreq = unsafe { device.get_buffer_memory_requirements(raw_buffer) };
        let alloc = device.allocator().allocate(&AllocationCreateDesc {
            name: "Buffer",
            requirements: memreq,
            location: if create_buffer.staging {
                MemoryLocation::CpuToGpu
            } else {
                MemoryLocation::GpuOnly
            },
            linear: true,
            allocation_scheme: AllocationScheme::DedicatedBuffer(raw_buffer),
        })?;
        unsafe { device.bind_buffer_memory(raw_buffer, alloc.memory(), alloc.offset()) }?;

        let buffer = GeneralBuffer {
            raw_buffer,
            staging: create_buffer.staging,
            type_id: TypeId::of::<T>(),
            alloc,
            item_size: type_size,
            size: create_buffer.len,
        };
        let raw_id = self.buffers.insert(buffer);
        Ok(GeneralBufferId {
            key_data: raw_id,
            item_size: type_size,
            len: create_buffer.len,
        })
    }
    /// Returns general (untyped) buffer if exists
    pub fn get_general_buffer(&self, general_id: GeneralBufferId) -> Option<&GeneralBuffer> {
        self.buffers.get(general_id.key_data)
    }
    /// Creates `StorageBuffer` and returns `StorageBufferId`
    pub fn create_storage_buffer<T: StorageData>(
        &mut self,
        device: &DeviceContext,
        create_buffer: CreateBuffer<T>,
    ) -> Result<StorageBufferId<T>, Box<dyn Error>> {
        let buffer = self.create_general_buffer(
            device,
            BufferUsage::Storage,
            T::min_size().into(),
            create_buffer,
        )?;
        Ok(StorageBufferId {
            gener: buffer,
            _marker: PhantomData {},
        })
    }
    /// Returns `StorageBuffer` from `StorageBufferId`
    pub fn get_storage_buffer<'a, T: StorageData>(
        &'a self,
        id: StorageBufferId<T>,
    ) -> Option<StorageBuffer<'a, T>> {
        Some(StorageBuffer {
            gener: self.get_general_buffer(id.gener)?,
            _marker: PhantomData {},
        })
    }

    /// Creates `UniformBuffer` and returns `UniformBufferId`
    pub fn create_uniform_buffer<T: UniformData>(
        &mut self,
        device: &DeviceContext,
        create_buffer: CreateBuffer<T>,
    ) -> Result<UniformBufferId<T>, Box<dyn Error>> {
        let buffer = self.create_general_buffer(
            device,
            BufferUsage::Uniform,
            T::min_size().into(),
            create_buffer,
        )?;

        Ok(UniformBufferId {
            gener: buffer,
            _marker: PhantomData {},
        })
    }

    /// Returns `UniformBuffer` from `UniformBufferId`
    pub fn get_uniform_buffer<'a, T: UniformData>(
        &'a self,
        id: UniformBufferId<T>,
    ) -> Option<UniformBuffer<'a, T>> {
        Some(UniformBuffer {
            gener: self.get_general_buffer(id.gener)?,
            _marker: PhantomData {},
        })
    }
    /// Creates `VertexBuffer` and returns `VertexBufferId`
    pub fn create_vertex_buffer<T: VertexData>(
        &mut self,
        device: &DeviceContext,
        create_buffer: CreateBuffer<T>,
    ) -> Result<VertexBufferId<T>, Box<dyn Error>> {
        let buffer = self.create_general_buffer(
            device,
            BufferUsage::Vertex,
            std::mem::size_of::<T>() as u64,
            create_buffer,
        )?;
        Ok(VertexBufferId {
            gener: buffer,
            _marker: PhantomData {},
        })
    }
    /// Returns `VertexBuffer` from `VertexBufferId`
    pub fn get_vertex_buffer<'a, T: VertexData>(
        &'a self,
        id: VertexBufferId<T>,
    ) -> Option<VertexBuffer<'a, T>> {
        Some(VertexBuffer {
            gener: self.get_general_buffer(id.gener)?,
            _marker: PhantomData {},
        })
    }

    /// Creates `IndexBuffer` and returns `IndexBufferId`
    pub fn create_index_buffer<T: IndexData>(
        &mut self,
        device: &DeviceContext,
        create_buffer: CreateBuffer<T>,
    ) -> Result<IndexBufferId<T>, Box<dyn Error>> {
        let buffer = self.create_general_buffer(
            device,
            BufferUsage::Index,
            std::mem::size_of::<T>() as u64,
            create_buffer,
        )?;
        Ok(IndexBufferId {
            gener: buffer,
            _marker: PhantomData {},
        })
    }
    /// Returns `IndexBuffer` from `IndexBufferId`
    pub fn get_index_buffer<'a, T: IndexData>(
        &'a self,
        id: IndexBufferId<T>,
    ) -> Option<IndexBuffer<'a, T>> {
        Some(IndexBuffer {
            gener: self.get_general_buffer(id.gener)?,
            _marker: PhantomData {},
        })
    }
}
slotmap::new_key_type! {
    /// Unique identifier to `GeneralBuffer` in a `BufferContainer`
    pub struct BufferId;
}

pub enum BufferUsage {
    Vertex,
    Index,
    Storage,
    Uniform,
    /// Just for copying data from CPU to GPU
    Staging,
}

/// Unique identifier to `GeneralBuffer` in a `BufferContainer` with extra information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneralBufferId {
    key_data: BufferId,
    len: u64,
    item_size: u64,
}

impl GeneralBufferId {
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.item_size == 0
    }
    pub fn item_size(&self) -> u64 {
        self.item_size
    }
    pub fn key_data(&self) -> BufferId {
        self.key_data
    }
}
/// Represents buffer with no type constraints
///
/// Use it carefully
pub struct GeneralBuffer {
    raw_buffer: ash::vk::Buffer,
    type_id: TypeId,
    alloc: Allocation,
    staging: bool,
    item_size: u64,
    size: u64,
}
impl GeneralBuffer {
    pub fn handle(&self) -> ash::vk::Buffer {
        self.raw_buffer
    }
    /// Returns `TypeId` of data stored in buffer
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns true if buffer is writeable from CPU
    ///
    /// If it's false then you can't write into buffer with code,
    /// but still you can upload data into different buffer and then copy its values into the other one
    pub fn staging(&self) -> bool {
        self.staging
    }

    /// Returns the size of buffer as count of items it contains
    pub fn size(&self) -> u64 {
        self.size
    }

    // Returns the size of an item in buffer
    pub fn item_size(&self) -> u64 {
        self.item_size
    }

    pub fn alloc(&self) -> &Allocation {
        &self.alloc
    }
}
/// General configuration parameters for creating a buffer
#[derive(Debug, Clone)]
pub struct CreateBuffer<T: GpuData> {
    len: u64,
    staging: bool,
    _marker: PhantomData<T>,
}
impl<T> CreateBuffer<T>
where
    T: GpuData,
{
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets length of buffer in respect to items
    pub fn len(mut self, len: u64) -> Self {
        self.len = len;
        self
    }
    /// If set to true marks buffer as writeable from CPU (might be slower)
    pub fn staging(mut self, staging: bool) -> Self {
        self.staging = staging;
        self
    }
}
impl<T> Default for CreateBuffer<T>
where
    T: GpuData,
{
    fn default() -> Self {
        Self {
            len: Default::default(),
            staging: Default::default(),
            _marker: Default::default(),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct StorageBufferId<T: StorageData> {
    gener: GeneralBufferId,
    _marker: PhantomData<T>,
}
impl<T> Deref for StorageBufferId<T>
where
    T: StorageData,
{
    type Target = GeneralBufferId;

    fn deref(&self) -> &Self::Target {
        &self.gener
    }
}
pub struct StorageBuffer<'a, T: StorageData> {
    gener: &'a GeneralBuffer,
    _marker: PhantomData<T>,
}
impl<'a, T> Deref for StorageBuffer<'a, T>
where
    T: StorageData,
{
    type Target = GeneralBuffer;

    fn deref(&self) -> &Self::Target {
        self.gener
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UniformBufferId<T: UniformData> {
    gener: GeneralBufferId,
    _marker: PhantomData<T>,
}
impl<T> Deref for UniformBufferId<T>
where
    T: UniformData,
{
    type Target = GeneralBufferId;

    fn deref(&self) -> &Self::Target {
        &self.gener
    }
}
pub struct UniformBuffer<'a, T: UniformData> {
    gener: &'a GeneralBuffer,
    _marker: PhantomData<T>,
}
impl<'a, T> Deref for UniformBuffer<'a, T>
where
    T: UniformData,
{
    type Target = GeneralBuffer;

    fn deref(&self) -> &Self::Target {
        self.gener
    }
}
#[derive(Debug, Clone, Copy)]
pub struct VertexBufferId<T: VertexData> {
    gener: GeneralBufferId,
    _marker: PhantomData<T>,
}
impl<T> Deref for VertexBufferId<T>
where
    T: VertexData,
{
    type Target = GeneralBufferId;

    fn deref(&self) -> &Self::Target {
        &self.gener
    }
}
pub struct VertexBuffer<'a, T: VertexData> {
    gener: &'a GeneralBuffer,
    _marker: PhantomData<T>,
}
impl<'a, T> Deref for VertexBuffer<'a, T>
where
    T: VertexData,
{
    type Target = GeneralBuffer;

    fn deref(&self) -> &Self::Target {
        self.gener
    }
}
pub struct IndexBufferId<T: IndexData> {
    gener: GeneralBufferId,
    _marker: PhantomData<T>,
}
impl<T> Deref for IndexBufferId<T>
where
    T: IndexData,
{
    type Target = GeneralBufferId;

    fn deref(&self) -> &Self::Target {
        &self.gener
    }
}
pub struct IndexBuffer<'a, T: IndexData> {
    gener: &'a GeneralBuffer,
    _marker: PhantomData<T>,
}
impl<'a, T> Deref for IndexBuffer<'a, T>
where
    T: IndexData,
{
    type Target = GeneralBuffer;

    fn deref(&self) -> &Self::Target {
        self.gener
    }
}
/// **Auto trait.**
pub trait GpuData: 'static {}

impl<T: 'static> GpuData for T {}

/// # Safety
/// **Not intented to be implemented by user.**
///
/// **Please use attribute `#[[storage_data]]`**
pub unsafe trait StorageData: encase::ShaderType + GpuData {}

/// # Safety
/// **Not intented to be implemented by user.**
///
/// **Please use attribute `#[[uniform_data]]`**
pub unsafe trait UniformData: encase::ShaderType + GpuData {}

/// # Safety
/// **Not intented to be implemented by user.**
///
/// please use one of these types: u32, u16
pub unsafe trait IndexData: GpuData {}
unsafe impl IndexData for u32 {}
unsafe impl IndexData for u16 {}
/// Attribute format
#[derive(Debug, Clone, Copy)]
pub enum AttributeFormat {
    //TODO:Extend for more data types
    Vec4F32,
    Vec3F32,
    Vec2F32,
    F32,
}
impl AttributeFormat {
    pub fn into_format(&self) -> Format {
        match self {
            AttributeFormat::Vec3F32 => Format::R32G32B32_SFLOAT,
            AttributeFormat::Vec2F32 => Format::R32G32_SFLOAT,
            AttributeFormat::F32 => Format::R32_SFLOAT,
            AttributeFormat::Vec4F32 => Format::R32G32B32A32_SFLOAT,
        }
    }
}
/// Struct that defines field for gpu
#[derive(Debug, Clone, Copy)]
pub struct VertexAttribute {
    /// How much bindings does this field take
    ///
    /// One binding can hold up to 16 bytes.
    ///
    /// So if taken into account for vec4 it would be 1, for mat4 it would be 4
    binding_size: usize,

    location: usize,
    /// At which byte does field start, given by trait `ToVertexAttribute` via argument
    offset: usize,
    /// What format is suitable for data-type
    format: AttributeFormat,
}

impl VertexAttribute {
    pub fn binding_size(&self) -> usize {
        self.binding_size
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn format(&self) -> AttributeFormat {
        self.format
    }

    pub fn location(&self) -> usize {
        self.location
    }
}

/// Trait that converts field into `VertexAttribute`
pub trait ToVertexAttribute {
    fn to_attrib(offset: usize, location: usize) -> Vec<VertexAttribute>;
}
impl ToVertexAttribute for [f32; 4] {
    fn to_attrib(offset: usize, location: usize) -> Vec<VertexAttribute> {
        vec![VertexAttribute {
            binding_size: 1,
            location,
            offset,
            format: AttributeFormat::Vec4F32,
        }]
    }
}
impl ToVertexAttribute for [f32; 3] {
    fn to_attrib(offset: usize, location: usize) -> Vec<VertexAttribute> {
        vec![VertexAttribute {
            binding_size: 1,
            offset,
            format: AttributeFormat::Vec3F32,
            location,
        }]
    }
}
impl ToVertexAttribute for [f32; 2] {
    fn to_attrib(offset: usize, location: usize) -> Vec<VertexAttribute> {
        vec![VertexAttribute {
            binding_size: 1,
            offset,
            format: AttributeFormat::Vec2F32,
            location,
        }]
    }
}
impl ToVertexAttribute for f32 {
    fn to_attrib(offset: usize, location: usize) -> Vec<VertexAttribute> {
        vec![VertexAttribute {
            binding_size: 1,
            offset,
            format: AttributeFormat::F32,
            location,
        }]
    }
}
/// # Safety
///
/// **Not intented to be implemented by user.**
///
/// **Please use `#[derive(VertexData)]`**
///
/// This trait defines how Vulkan will be treating vertex buffer
///
/// Trait only works with data types that implement `ToVertexAttribute`
pub unsafe trait VertexData: GpuData {
    fn layout_info() -> Vec<Vec<VertexAttribute>>;
}

unsafe impl VertexData for () {
    fn layout_info() -> Vec<Vec<VertexAttribute>> {
        Vec::new()
    }
}
