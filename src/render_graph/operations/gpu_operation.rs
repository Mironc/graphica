use ash::vk::{AccessFlags, CommandBuffer, ImageLayout, PipelineStageFlags};

use crate::{
    device::DeviceContext,
    render_graph::{
        operations::{
            custom::CustomOp,
            draw_call::DrawCall,
            upload::{UploadBufferOp, UploadImageOp},
        },
        resource::ResourceId,
        resource_state::{ResourceAccess, ResourceState, ResourceUsage},
    },
    rendering::renderer_bundle::RendererBundle,
    swapchain::FrameImage,
};

#[derive(Debug, Clone)]
pub enum Operation {
    DrawCall(DrawCall),
    WriteBuffer(UploadBufferOp),
    UploadImage(UploadImageOp),
    Present(FrameImage),
    Custom(Box<dyn CustomOp>),
}
impl Operation {
    pub fn resource_state(&self, bundle: &RendererBundle) -> Option<Vec<ResourceState>> {
        match self {
            Operation::DrawCall(draw_call) => match draw_call {
                DrawCall::Direct { draw_param } => draw_param.resource_state(bundle),
            },
            Operation::WriteBuffer(write_buffer_op) => write_buffer_op.resource_state(bundle),
            Operation::Present(frame) => {
                bundle.texture_container.get_frameimage(frame).map(|tex| {
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
                })
            }
            Operation::UploadImage(upload_image_op) => upload_image_op.resource_state(bundle),
            Operation::Custom(op) => op.resource_state(bundle),
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
            Operation::Custom(custom_op) => {
                custom_op.execute(device, command_buffer, bundle);
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
            Operation::Custom(_) => "Custom",
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

#[derive(Debug, Clone)]
pub enum OperationError {}
