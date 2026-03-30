use ash::vk::{
    AccessFlags, BufferImageCopy, CommandBuffer, ImageAspectFlags, ImageLayout,
    ImageSubresourceLayers, Offset3D, PipelineStageFlags,
};
use encase::DynamicUniformBuffer;
use image::{EncodableLayout, RgbaImage};

use crate::{
    device::DeviceContext,
    render_graph::{
        operations::draw_call::DrawCall,
        resource::ResourceId,
        resource_state::{ResourceAccess, ResourceState, ResourceUsage},
    },
    rendering::{
        buffer_container::{
            BufferUsage, CreateBuffer, GeneralBufferId, UniformBufferId, UniformData,
            VertexBufferId, VertexData,
        },
        renderer_bundle::RendererBundle,
        texture_container::TextureId,
    },
    swapchain::FrameData,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    DrawCall(DrawCall),
    WriteBuffer(WriteBufferOp),
    UploadImage(UploadImageOp),
    Present(FrameData),
}
impl Operation {
    pub fn resource_state(&self, bundle: &RendererBundle) -> Option<Vec<ResourceState>> {
        match self {
            Operation::DrawCall(draw_call) => match draw_call {
                DrawCall::Direct { draw_param } => draw_param.resource_state(bundle),
            },
            Operation::WriteBuffer(write_buffer_op) => {
                let res = ResourceId::Buffer(write_buffer_op.buff);
                Some(
                    [ResourceState::new(
                        res,
                        ResourceUsage::Buffer(
                            PipelineStageFlags::HOST,
                            write_buffer_op.offset_bytes,
                            write_buffer_op.data.len() as u64,
                            AccessFlags::HOST_WRITE,
                            ResourceAccess::Write,
                        ),
                    )]
                    .to_vec(),
                )
            }
            Operation::Present(frame) => bundle
                .texture_container
                .get_frameimage(frame.image())
                .map(|tex| {
                    [ResourceState::new(
                        ResourceId::Texture(tex.0),
                        ResourceUsage::Texture(
                            ImageLayout::PRESENT_SRC_KHR,
                            PipelineStageFlags::BOTTOM_OF_PIPE,
                            AccessFlags::empty(),
                            ResourceAccess::Read,
                        ),
                    )]
                    .to_vec()
                }),
            Operation::UploadImage(upload_image_op) => {
                let res = ResourceId::Texture(upload_image_op.texture_id);
                Some(
                    [ResourceState::new(
                        res,
                        ResourceUsage::Texture(
                            ImageLayout::TRANSFER_DST_OPTIMAL,
                            PipelineStageFlags::TRANSFER,
                            AccessFlags::TRANSFER_WRITE,
                            ResourceAccess::Read,
                        ),
                    )]
                    .to_vec(),
                )
            }
        }
    }
    pub fn execute(
        &self,
        device: &DeviceContext,
        command_buffer: CommandBuffer,
        bundle: &mut RendererBundle,
    ) {
        match self {
            Operation::DrawCall(draw_call) => {
                draw_call.execute(bundle, command_buffer, device);
            }
            Operation::WriteBuffer(write_buffer_op) => {
                write_buffer_op.execute(bundle, device);
            }
            Operation::Present(_) => {
                //nothing cuz we need only to sync image
            }
            Operation::UploadImage(upload_image_op) => {
                upload_image_op.execute(command_buffer, bundle, device);
            }
        }
    }
}
#[cfg(feature = "graph-visualize")]
impl Operation {
    pub fn dot_type_label(&self) -> String {
        match self {
            Operation::DrawCall(_) => "DrawCall",
            Operation::WriteBuffer(_) => "WriteBuffer",
            Operation::UploadImage(_) => "WriteImage",
            Operation::Present(_) => "PresentFrame",
        }
        .to_owned()
    }
    pub fn fmt_dot(&self, bundle: &RendererBundle, label: Option<&str>) -> String {
        let type_label = self.dot_type_label();
        let additional_info = if let Some(states) = self.resource_state(bundle) {
            let mut reads = String::new();
            let mut writes = String::new();
            for state in states.iter() {
                use std::fmt::Write;
                match state.resource_usage().resource_access() {
                    ResourceAccess::Read => {
                        _ = write!(&mut reads, "{} | ", state.resource_id().fmt_dot(bundle));
                    }
                    ResourceAccess::Write => {
                        _ = write!(&mut writes, "{} | ", state.resource_id().fmt_dot(bundle));
                    }
                    ResourceAccess::ReadWrite => {
                        _ = write!(&mut reads, "{} | ", state.resource_id().fmt_dot(bundle));
                        _ = write!(&mut writes, "{} | ", state.resource_id().fmt_dot(bundle));
                    }
                }
            }
            let reads = reads.trim().trim_end_matches('|');
            let writes = writes.trim().trim_end_matches('|');
            let mut info = String::new();
            use std::fmt::Write;
            if !reads.is_empty() {
                _ = write!(&mut info, "| {{Reads | {} }}", reads);
            }
            if !writes.is_empty() {
                _ = write!(&mut info, "| {{Writes | {} }}", writes);
            }
            info
        } else {
            "Failed to acquire extra info due to faulty data".to_owned()
        };
        if let Some(label) = label {
            format!(
                "shape=record, label=\"{{ {} - '{}' {} }}\"",
                type_label, label, additional_info
            )
        } else {
            format!(
                "shape=record, label=\"{{ {} {} }}\"",
                type_label, additional_info
            )
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteBufferOp {
    buff: GeneralBufferId,
    data: Vec<u8>,
    //len_bytes: u64,
    offset_bytes: u64,
}
impl WriteBufferOp {
    pub fn vertex_buffer<V: VertexData>(
        buff: VertexBufferId<V>,
        data: Vec<V>,
        offset: u64,
    ) -> Option<Self> {
        // Writes to outside of binded memory -> return None
        if offset + (data.len() as u64) > buff.len() {
            return None;
        }

        let v_size = std::mem::size_of::<V>();
        let byte_len = data.len() * v_size;
        let offset_bytes = offset * v_size as u64;

        let mut as_u8 = Vec::with_capacity(byte_len);

        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, as_u8.as_mut_ptr(), byte_len);
            as_u8.set_len(byte_len);
        }
        Some(Self {
            buff: *buff,
            data: as_u8,
            offset_bytes,
        })
    }
    pub fn uniform_buffer<U: UniformData + encase::internal::WriteInto>(
        buff: UniformBufferId<U>,
        data: Vec<U>,
        offset: u64,
    ) -> Option<Self> {
        // Writes to outside of binded memory -> return None
        if offset + (data.len() as u64) > buff.len() {
            return None;
        }

        let rhs: u64 = U::min_size().into();
        let offset_bytes = offset * rhs;
        let mut buf = Vec::new();
        let mut writer = DynamicUniformBuffer::new(&mut buf);
        for t in data.iter() {
            writer.write(&t).unwrap();
        }
        Some(Self {
            buff: *buff,
            data: buf,
            offset_bytes,
        })
    }
    pub fn execute(&self, bundle: &RendererBundle, device: &DeviceContext) {
        let buff = bundle
            .buffer_container
            .get_general_buffer(self.buff)
            .unwrap();
        let allocation = buff.alloc();
        let size = (std::mem::size_of_val(&self.data) * self.data.len()) as u64;

        if let Some(ptr) = allocation.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.data.as_ptr(),
                    ptr.as_ptr() as *mut u8,
                    size as usize,
                );
            }
        } else {
            log::error!("Buffer does not support mapping");
        }
        unsafe {
            device
                .flush_mapped_memory_ranges(&[ash::vk::MappedMemoryRange::default()
                    .memory(allocation.memory())
                    .offset(allocation.offset())
                    .size(allocation.size())])
                .unwrap()
        };
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadImageOp {
    image: RgbaImage,
    texture_id: TextureId,
}

impl UploadImageOp {
    pub fn new(image: RgbaImage, texture_id: TextureId) -> Self {
        Self { image, texture_id }
    }
    pub fn execute(
        &self,
        command_buffer: CommandBuffer,
        bundle: &mut RendererBundle,
        device: &DeviceContext,
    ) -> Option<()> {
        const PX_SIZE: u64 = 4;
        let texture = bundle.texture_container.get_image(self.texture_id)?;
        let px_count = (self.image.dimensions().0 * self.image.dimensions().1) as u64;
        let buff = bundle
            .buffer_container
            .create_general_buffer(
                device,
                BufferUsage::Staging,
                PX_SIZE,
                CreateBuffer::<()>::new().len(px_count).staging(true),
            )
            .unwrap();
        let buff = bundle.buffer_container.get_general_buffer(buff)?;
        let allocation = buff.alloc();
        let size = px_count * PX_SIZE;
        let data = self.image.as_bytes();

        if let Some(ptr) = allocation.mapped_ptr() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    ptr.as_ptr() as *mut u8,
                    size as usize,
                );
            }
        } else {
            log::error!("Buffer does not support mapping");
        }
        let buffer_image_copy = [BufferImageCopy::default()
            .buffer_row_length(texture.dimensions().width)
            .buffer_image_height(texture.dimensions().height)
            .buffer_offset(0)
            .image_extent(texture.dimensions())
            .image_offset(Offset3D::default())
            .image_subresource(
                ImageSubresourceLayers::default()
                    .mip_level(1)
                    .mip_level(0)
                    .layer_count(1)
                    .aspect_mask(ImageAspectFlags::COLOR),
            )];
        unsafe {
            device
                .flush_mapped_memory_ranges(&[ash::vk::MappedMemoryRange::default()
                    .memory(allocation.memory())
                    .offset(allocation.offset())
                    .size(allocation.size())])
                .unwrap();
            device.cmd_copy_buffer_to_image(
                command_buffer,
                buff.handle(),
                texture.handle(),
                ImageLayout::TRANSFER_DST_OPTIMAL,
                &buffer_image_copy,
            );
        };
        Some(())
    }
}
