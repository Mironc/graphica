use ash::vk::{
    AccessFlags, AttachmentDescription, AttachmentDescriptionFlags, AttachmentLoadOp,
    AttachmentReference, AttachmentStoreOp, ImageLayout, PipelineBindPoint, SUBPASS_EXTERNAL,
    SampleCountFlags, SubpassDependency, SubpassDescription,
};
use log::warn;
use std::collections::HashMap;

use crate::{device::DeviceContext, rendering::texture_container::TextureFormat};
#[derive(Debug, Default)]
pub struct RenderPassContainer {
    render_passes: HashMap<RenderPassDescription, RenderPass>,
}
impl RenderPassContainer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_renderpass(
        &mut self,
        device_ctx: &DeviceContext,
        mut create: RenderPassDescription,
    ) -> Option<()> {
        let mut attachments = Vec::new();
        for (i, attachment) in create.attachments.iter_mut().enumerate() {
            //let new_initial_layout =
            // if matches!(attachment.load_op, LoadOption::Clear | LoadOption::DontCare) {
            //     ImageLayout::UNDEFINED
            // } else {
            //     attachment.initial_layout
            // };
            let attachment_desc = AttachmentDescription::default()
                .flags(AttachmentDescriptionFlags::empty())
                .format(attachment.format.to_image_format())
                .samples(SampleCountFlags::TYPE_1)
                .initial_layout(attachment.initial_layout)
                .final_layout(attachment.final_layout)
                .load_op(attachment.load_op.to_loadop())
                .store_op(attachment.store_op.to_storeop())
                .stencil_load_op(attachment.stencil_load.to_loadop())
                .stencil_store_op(attachment.stencil_store.to_storeop());
            attachments.push(attachment_desc);
        }
        let mut input_attach = Vec::new();
        let mut color_refs = Vec::new();
        let mut depth_ref = None;
        for &attach_ref in create.subpass.read_attachments.iter() {
            if let Some(attach) = create.attachments.get(attach_ref) {
                let layout = optimal_image_layout(attach, AttachmentUsage::Read);
                input_attach.push(
                    AttachmentReference::default()
                        .attachment(attach_ref as u32)
                        .layout(layout),
                );
            }
        }
        for &attach_ref in create.subpass.rw_attachments.iter() {
            if let Some(attach) = create.attachments.get(attach_ref) {
                let layout = optimal_image_layout(attach, AttachmentUsage::ReadWrite);
                if attach.format.is_color() {
                    color_refs.push(
                        AttachmentReference::default()
                            .attachment(attach_ref as u32)
                            .layout(layout),
                    );
                }
                if attach.format.is_depth() || attach.format.is_depth_stencil() {
                    if depth_ref.is_some() {
                        warn!("Depth attachment was provided twice")
                    }
                    depth_ref = Some(
                        AttachmentReference::default()
                            .attachment(attach_ref as u32)
                            .layout(layout),
                    );
                }
                input_attach.push(
                    AttachmentReference::default()
                        .attachment(attach_ref as u32)
                        .layout(layout),
                );
            }
        }
        for &attach_ref in create.subpass.write_attachments.iter() {
            if let Some(attach) = create.attachments.get(attach_ref) {
                let layout = optimal_image_layout(attach, AttachmentUsage::Write);
                if attach.format.is_color() {
                    color_refs.push(
                        AttachmentReference::default()
                            .attachment(attach_ref as u32)
                            .layout(layout),
                    );
                }
                if attach.format.is_depth() || attach.format.is_depth_stencil() {
                    if depth_ref.is_some() {
                        warn!("Depth attachment was provided twice")
                    }
                    depth_ref = Some(
                        AttachmentReference::default()
                            .attachment(attach_ref as u32)
                            .layout(layout),
                    );
                }
            }
        }
        let mut subpass_desc = SubpassDescription::default()
            .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)
            .input_attachments(&input_attach);
        if let Some(depth_ref) = depth_ref.as_ref() {
            subpass_desc = subpass_desc.depth_stencil_attachment(depth_ref);
        }
        //let dependencies = [SubpassDependency::default().src_subpass(SUBPASS_EXTERNAL).dst_subpass(1).src_stage_mask(src_stage_mask)]
        let binding = [subpass_desc];
        let create_renderpass = ash::vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&binding);
        match unsafe { device_ctx.create_render_pass(&create_renderpass, None) } {
            Ok(handle) => {
                // clone here is acceptable
                self.render_passes.insert(
                    create.clone(),
                    RenderPass {
                        handle,
                        description: create,
                    },
                );
                Some(())
            }
            Err(e) => {
                log::error!("error occured while creating renderpass: {}", e);
                return None;
            }
        }
    }
    pub fn get_render_pass(&self, description: &RenderPassDescription) -> Option<&RenderPass> {
        self.render_passes.get(&description)
    }
}

fn optimal_image_layout(attachment: &RenderPassAttachment, usage: AttachmentUsage) -> ImageLayout {
    match usage {
        AttachmentUsage::Read => {
            if attachment.format.is_color() {
                return ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            }
            if attachment.format.is_depth() {
                return ImageLayout::DEPTH_READ_ONLY_OPTIMAL;
            }
            ImageLayout::DEPTH_ATTACHMENT_STENCIL_READ_ONLY_OPTIMAL
        }
        AttachmentUsage::Write => {
            if attachment.format.is_color() {
                return ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
            }
            if attachment.format.is_depth() {
                return ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
            }
            return ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        }
        AttachmentUsage::ReadWrite => {
            if attachment.format.is_color() {
                return ImageLayout::GENERAL;
            }
            if attachment.format.is_depth() {
                return ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
            }
            return ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;
        }
    }
}
#[derive(Debug, Clone)]
pub struct RenderPass {
    handle: ash::vk::RenderPass,
    description: RenderPassDescription,
}
impl RenderPass {
    pub fn handle(&self) -> ash::vk::RenderPass {
        self.handle
    }

    pub fn description(&self) -> &RenderPassDescription {
        &self.description
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderPassDescription {
    pub attachments: Vec<RenderPassAttachment>,
    pub subpass: SubPass,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StoreOption {
    /// Attachment data gets preserved
    Store,
    /// Attachment don't gets saved into texture (useful for temporary data)
    DontCare,
}
impl StoreOption {
    pub fn to_storeop(&self) -> AttachmentStoreOp {
        match self {
            StoreOption::Store => AttachmentStoreOp::STORE,
            StoreOption::DontCare => AttachmentStoreOp::DONT_CARE,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LoadOption {
    /// Attachment gets cleared with given color
    Clear,
    /// Attachment data gets loaded from initial texture
    Load,
    /// Do not loads or creates texture, so attachment might be filled with garbage memory
    DontCare,
}
impl LoadOption {
    pub fn to_loadop(&self) -> AttachmentLoadOp {
        match self {
            LoadOption::Clear => AttachmentLoadOp::CLEAR,
            LoadOption::Load => AttachmentLoadOp::LOAD,
            LoadOption::DontCare => AttachmentLoadOp::DONT_CARE,
        }
    }
}
/// Configuration for attachment
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderPassAttachment {
    pub load_op: LoadOption,
    pub store_op: StoreOption,
    pub format: TextureFormat,
    pub initial_layout: ImageLayout,
    pub final_layout: ImageLayout,
    pub stencil_store: StoreOption,
    pub stencil_load: LoadOption,
}
impl RenderPassAttachment {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn load_op(mut self, load_op: LoadOption) -> Self {
        self.load_op = load_op;
        self
    }
    pub fn store_op(mut self, store_op: StoreOption) -> Self {
        self.store_op = store_op;
        self
    }
    pub fn format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }
    pub fn initial_layout(mut self, initial_layout: ImageLayout) -> Self {
        self.initial_layout = initial_layout;
        self
    }
    pub fn final_layout(mut self, final_layout: ImageLayout) -> Self {
        self.final_layout = final_layout;
        self
    }
    pub fn stencil_load_op(mut self, load_op: LoadOption) -> Self {
        self.stencil_load = load_op;
        self
    }
    pub fn stencil_store_op(mut self, store_op: StoreOption) -> Self {
        self.stencil_store = store_op;
        self
    }
    pub fn access_flags(&self, usage: AttachmentUsage) -> AccessFlags {
        let mut access_flags = AccessFlags::empty();
        match usage {
            AttachmentUsage::Read => {
                if self.format.is_depth() {
                    access_flags |= AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ;
                } else {
                    access_flags |= AccessFlags::COLOR_ATTACHMENT_READ;
                }
                access_flags |= AccessFlags::INPUT_ATTACHMENT_READ;
            }
            AttachmentUsage::Write => {
                if self.format.is_depth() {
                    access_flags |= AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                        | AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ;
                } else {
                    access_flags |=
                        AccessFlags::COLOR_ATTACHMENT_WRITE | AccessFlags::COLOR_ATTACHMENT_READ;
                }
            }
            AttachmentUsage::ReadWrite => {
                if self.format.is_depth() {
                    access_flags |= AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                        | AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE;
                } else {
                    access_flags |=
                        AccessFlags::COLOR_ATTACHMENT_READ | AccessFlags::COLOR_ATTACHMENT_WRITE;
                }
                access_flags |= AccessFlags::INPUT_ATTACHMENT_READ
            }
        }
        access_flags
    }
}
impl Default for RenderPassAttachment {
    fn default() -> Self {
        Self {
            load_op: LoadOption::DontCare,
            store_op: StoreOption::DontCare,
            format: Default::default(),
            initial_layout: Default::default(),
            final_layout: Default::default(),
            stencil_store: StoreOption::DontCare,
            stencil_load: LoadOption::DontCare,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SubPass {
    read_attachments: Vec<usize>,
    write_attachments: Vec<usize>,
    rw_attachments: Vec<usize>,
}

impl SubPass {
    pub fn new(
        read_attachments: Vec<usize>,
        write_attachments: Vec<usize>,
        rw_attachments: Vec<usize>,
    ) -> Self {
        Self {
            read_attachments,
            write_attachments,
            rw_attachments,
        }
    }

    pub fn read_attachments(&self) -> &[usize] {
        &self.read_attachments
    }

    pub fn write_attachments(&self) -> &[usize] {
        &self.write_attachments
    }

    pub fn rw_attachments(&self) -> &[usize] {
        &self.rw_attachments
    }
}
pub enum AttachmentUsage {
    Read,
    Write,
    ReadWrite,
}
// Too much hassle with subpasses :(
// Maybe later
/* impl RenderPassContainer {
    pub fn create_renderpass(
        &mut self,
        device_ctx: &DeviceContext,
        create: RenderPassDescription,
    ) -> Option<()> {
        if create.subpasses.len()==0{
            warn!("RenderPass with no subpasses is not created");
            return None;
        }
        let mut attachments = Vec::new();
        let mut attachment_last_usage = Vec::with_capacity(create.attachments.len());
        for (i, attachment) in create.attachments.iter().enumerate() {
            attachment_last_usage.push(
                create
                    .subpasses
                    .iter()
                    .rposition(|x| {
                        !(x.read_attachments.contains(&i)
                            || x.write_attachments.contains(&i)
                            || x.rw_attachments.contains(&i))
                    })
                    .unwrap_or_default(),
            );
            let attachment_desc = AttachmentDescription::default()
                .flags(AttachmentDescriptionFlags::empty())
                .format(attachment.format.to_image_format())
                .initial_layout(attachment.initial_layout)
                .final_layout(attachment.final_layout)
                .load_op(attachment.load_op.to_loadop())
                .store_op(attachment.store_op.to_storeop())
                .stencil_load_op(attachment.stencil_load.to_loadop())
                .stencil_store_op(attachment.stencil_store.to_storeop());
            attachments.push(attachment_desc);
        }
        let subpasses = Vec::new();
        for (i, subpass) in create.subpasses.iter().enumerate() {
            let mut input_attach = Vec::new();
            let mut color_refs = Vec::new();
            let mut depth_ref = None;
            for &attach_ref in subpass.read_attachments.iter() {
                if let Some(attach) = create.attachments.get(attach_ref) {
                    let layout = optimal_image_layout(attach, AttachmentUsage::Read);
                    if attach.format.is_color() {
                        color_refs.push(
                            AttachmentReference::default()
                                .attachment(attach_ref as u32)
                                .layout(layout),
                        );
                    }
                    if attach.format.is_depth() || attach.format.is_depth_stencil() {
                        if depth_ref.is_some() {
                            warn!("Depth attachment was provided twice")
                        }
                        depth_ref = Some(
                            AttachmentReference::default()
                                .attachment(attach_ref as u32)
                                .layout(layout),
                        );
                    }
                }
            }
            for &attach_ref in subpass.rw_attachments.iter() {
                if let Some(attach) = create.attachments.get(attach_ref) {
                    let layout = optimal_image_layout(attach, AttachmentUsage::ReadWrite);
                    if attach.format.is_color() {
                        color_refs.push(
                            AttachmentReference::default()
                                .attachment(attach_ref as u32)
                                .layout(layout),
                        );
                    }
                    if attach.format.is_depth() || attach.format.is_depth_stencil() {
                        if depth_ref.is_some() {
                            warn!("Depth attachment was provided twice")
                        }
                        depth_ref = Some(
                            AttachmentReference::default()
                                .attachment(attach_ref as u32)
                                .layout(layout),
                        );
                    }
                    input_attach.push(
                        AttachmentReference::default()
                            .attachment(attach_ref as u32)
                            .layout(layout),
                    );
                }
            }
            for &attach_ref in subpass.write_attachments.iter() {
                if let Some(attach) = create.attachments.get(attach_ref) {
                    let layout = optimal_image_layout(attach, AttachmentUsage::Write);
                    let attach_ref = AttachmentReference::default()
                        .attachment(attach_ref as u32)
                        .layout(layout);
                    input_attach.push(attach_ref);
                }
            }
            let preserve = attachment_last_usage
                .iter()
                .filter(|&x| x.eq(&i))
                .filter(|&x| {
                    !(subpass.read_attachments.contains(x)
                        || subpass.rw_attachments.contains(x)
                        || subpass.write_attachments.contains(x))
                })
                .map(|x| *x as u32)
                .collect::<Vec<u32>>();
            let mut subpass_desc = SubpassDescription::default()
                .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_refs)
                .input_attachments(&input_attach)
                .preserve_attachments(&preserve);
            if let Some(depth_ref) = depth_ref {
                subpass_desc = subpass_desc.depth_stencil_attachment(&depth_ref);
            }
        }
        let mut dependencies = Vec::new();
        //TODO: add more sophisticated solution for dependencies
        dependencies.push(
            SubpassDependency::default()
                .src_subpass(ash::vk::SUBPASS_EXTERNAL)
                .src_stage_mask(PipelineStageFlags::BOTTOM_OF_PIPE)
                .dst_stage_mask(PipelineStageFlags::TOP_OF_PIPE)
                .src_access_mask(AccessFlags::empty())
                .dst_access_mask(
                    AccessFlags::COLOR_ATTACHMENT_READ | AccessFlags::COLOR_ATTACHMENT_WRITE,
                ),
        );
        for i in 1..attachments.len() {
            let dep = SubpassDependency::default()
                .src_subpass((i - 1) as u32)
                .dst_subpass(i as u32)
                .src_stage_mask(PipelineStageFlags::BOTTOM_OF_PIPE)
                .dst_stage_mask(PipelineStageFlags::TOP_OF_PIPE)
                .src_access_mask(AccessFlags::)
                .dependency_flags(DependencyFlags::BY_REGION);
            dependencies.push(dep);
        }
        let create_renderpass = ash::vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        Some(self.render_passes.insert((), ())?)
    }
} */
