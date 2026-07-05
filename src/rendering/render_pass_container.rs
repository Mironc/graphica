use ash::vk::{
    AccessFlags, AttachmentDescription, AttachmentDescriptionFlags, AttachmentLoadOp,
    AttachmentReference, AttachmentStoreOp, ImageLayout, PipelineBindPoint, PipelineStageFlags,
    SUBPASS_EXTERNAL, SampleCountFlags, SubpassDependency, SubpassDescription,
};
use log::warn;
use slotmap::SlotMap;
use std::{collections::HashMap, error::Error};

use crate::{
    device::DeviceContext,
    rendering::{
        framebuffer_container::{FramebufferContainer, FramebufferId},
        shader_container::OutputTypes,
        texture_container::{TextureContainer, TextureFormat},
    },
};
#[derive(Debug, Default)]
pub struct RenderPassContainer {
    render_pass_layouts: SlotMap<RenderPassId, OutputTypes>,
    render_passes: HashMap<(RenderPassId, Vec<RenderPassAttachment>, RenderPassSync), RenderPass>,
    /// These render passes are a bit more dull as they are not really handled by frame graph
    /// Useful when you need to use them for something like ImGui, but not really perfomant
    headless_passes: HashMap<FramebufferId, ash::vk::RenderPass>,
}
impl RenderPassContainer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn render_pass_layout(&mut self, output_types: OutputTypes) -> RenderPassId {
        if let Some(id) = self
            .render_pass_layouts
            .iter()
            .find(|x| x.1.attachments_compatible(&output_types))
        {
            return id.0;
        }
        self.render_pass_layouts.insert(output_types)
    }
    pub fn get_render_pass_layout(&self, id: RenderPassId) -> Option<&OutputTypes> {
        self.render_pass_layouts.get(id)
    }
    pub fn get_concrete_render_pass(
        &mut self,
        device_ctx: &DeviceContext,
        render_pass_id: RenderPassId,
        render_attachments: Vec<RenderPassAttachment>,
        sync: RenderPassSync,
    ) -> Result<&RenderPass, Box<dyn Error>> {
        let mut key = (render_pass_id, render_attachments, sync);
        Ok(self.render_passes.entry(key.clone()).or_insert_with(|| {
            let mut attachments: Vec<AttachmentDescription> = Vec::new();
            for attachment in key.1.iter_mut() {
                let new_initial_layout =
                    if matches!(attachment.load_op, LoadOption::Clear | LoadOption::DontCare) {
                        ImageLayout::UNDEFINED
                    } else {
                        attachment.initial_layout
                    };
                if new_initial_layout == ImageLayout::UNDEFINED {
                    attachment.load_op = LoadOption::DontCare;
                }
                let attachment_desc = AttachmentDescription::default()
                    .flags(AttachmentDescriptionFlags::empty())
                    .format(attachment.format.to_image_format())
                    .samples(SampleCountFlags::TYPE_1)
                    .initial_layout(new_initial_layout)
                    .final_layout(attachment.final_layout)
                    .load_op(attachment.load_op.to_loadop())
                    .store_op(attachment.store_op.to_storeop())
                    .stencil_load_op(attachment.stencil_load.to_loadop())
                    .stencil_store_op(attachment.stencil_store.to_storeop());
                attachments.push(attachment_desc);
            }
            let input_attach = Vec::new();
            let mut color_refs = Vec::new();
            let mut depth_ref = None;
            let mut ps = PipelineStageFlags::empty();
            let mut ac = AccessFlags::empty();
            for (attach_ref, attach) in key.1.iter().enumerate() {
                let layout = optimal_image_layout(attach.format, AttachmentUsage::Write);
                if attach.format.is_color() {
                    color_refs.push(
                        AttachmentReference::default()
                            .attachment(attach_ref as u32)
                            .layout(layout),
                    );
                    ps |= PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
                    ac |= AccessFlags::COLOR_ATTACHMENT_WRITE;
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
                    ps |= PipelineStageFlags::LATE_FRAGMENT_TESTS
                        | PipelineStageFlags::EARLY_FRAGMENT_TESTS;
                    ac |= AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                        | AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ;
                }
            }
            let mut subpass_desc = SubpassDescription::default()
                .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
                .color_attachments(&color_refs)
                .input_attachments(&input_attach);
            if let Some(depth_ref) = depth_ref.as_ref() {
                subpass_desc = subpass_desc.depth_stencil_attachment(depth_ref);
            }

            let dependencies = [
                SubpassDependency::default()
                    .src_subpass(SUBPASS_EXTERNAL)
                    .src_stage_mask(sync.pipeline_stage_from)
                    .dst_access_mask(sync.access_flags_from)
                    .dst_subpass(0)
                    .dst_stage_mask(ps)
                    .dst_access_mask(ac),
                SubpassDependency::default()
                    .src_subpass(0)
                    .src_stage_mask(ps)
                    .src_access_mask(ac)
                    .dst_subpass(SUBPASS_EXTERNAL)
                    .dst_stage_mask(sync.pipeline_stage_to)
                    .dst_access_mask(sync.access_flags_to),
            ];
            let binding = [subpass_desc];
            let create_renderpass = ash::vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&binding)
                .dependencies(&dependencies);
            let handle =
                unsafe { device_ctx.create_render_pass(&create_renderpass, None) }.unwrap();
            RenderPass {
                handle,
                attachments: key.1.clone(),
            }
        }))
    }
}
impl RenderPassContainer {
    pub fn headless_render_pass(
        &mut self,
        device_ctx: &DeviceContext,
        texture_container: &TextureContainer,
        framebuffer_container: &FramebufferContainer,
        framebuffer_id: FramebufferId,
    ) -> Result<ash::vk::RenderPass, Box<dyn Error>> {
        let entry = self.headless_passes.entry(framebuffer_id);
        match entry {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => {
                Ok(*occupied_entry.get())
            }
            std::collections::hash_map::Entry::Vacant(vacant_entry) => {
                let framebuffer_attachments = framebuffer_container
                    .get_framebuffer_layout(framebuffer_id)
                    .ok_or(<Box<dyn Error>>::from(format!(
                        "No such framebuffer with id:{:?}",
                        framebuffer_id
                    )))?;
                let mut attachments: Vec<AttachmentDescription> = Vec::new();
                for attachment in framebuffer_attachments.iter() {
                    let texture = texture_container
                        .get_image(attachment.texture())
                        .ok_or(<Box<dyn Error>>::from(format!(
                            "No such framebuffer with id:{:?}",
                            framebuffer_id
                        )))?;
                    let layout =
                        optimal_image_layout(texture.texture_format(), AttachmentUsage::Write);
                    let attachment_desc = AttachmentDescription::default()
                        .flags(AttachmentDescriptionFlags::empty())
                        .format(texture.texture_format().to_image_format())
                        .samples(SampleCountFlags::TYPE_1)
                        .initial_layout(layout)
                        .final_layout(layout)
                        .load_op(LoadOption::Load.to_loadop())
                        .store_op(StoreOption::Store.to_storeop())
                        .stencil_load_op(LoadOption::Load.to_loadop())
                        .stencil_store_op(StoreOption::Store.to_storeop());
                    attachments.push(attachment_desc);
                }
                let input_attach = Vec::new();
                let mut color_refs = Vec::new();
                let mut depth_ref = None;
                let mut ps = PipelineStageFlags::empty();
                let mut ac = AccessFlags::empty();
                for (attach_ref, attach) in framebuffer_attachments.iter().enumerate() {
                    let texture = texture_container.get_image(attach.texture()).unwrap();

                    let layout =
                        optimal_image_layout(texture.texture_format(), AttachmentUsage::Write);
                    if texture.texture_format().is_color() {
                        color_refs.push(
                            AttachmentReference::default()
                                .attachment(attach_ref as u32)
                                .layout(layout),
                        );
                        ps |= PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
                        ac |= AccessFlags::COLOR_ATTACHMENT_WRITE;
                    }
                    if texture.texture_format().is_depth()
                        || texture.texture_format().is_depth_stencil()
                    {
                        if depth_ref.is_some() {
                            warn!("Depth attachment was provided twice")
                        }
                        depth_ref = Some(
                            AttachmentReference::default()
                                .attachment(attach_ref as u32)
                                .layout(layout),
                        );
                        ps |= PipelineStageFlags::LATE_FRAGMENT_TESTS
                            | PipelineStageFlags::EARLY_FRAGMENT_TESTS;
                        ac |= AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                            | AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ;
                    }
                }
                let mut subpass_desc = SubpassDescription::default()
                    .pipeline_bind_point(PipelineBindPoint::GRAPHICS)
                    .color_attachments(&color_refs)
                    .input_attachments(&input_attach);
                if let Some(depth_ref) = depth_ref.as_ref() {
                    subpass_desc = subpass_desc.depth_stencil_attachment(depth_ref);
                }
                let subpass_bind = [subpass_desc];
                let dependencies = [];
                let create_renderpass = ash::vk::RenderPassCreateInfo::default()
                    .attachments(&attachments)
                    .subpasses(&subpass_bind)
                    .dependencies(&dependencies);
                let handle =
                    unsafe { device_ctx.create_render_pass(&create_renderpass, None) }.unwrap();
                vacant_entry.insert(handle);
                Ok(handle)
            }
        }
    }
}
slotmap::new_key_type! {
    pub struct RenderPassId;
}
fn optimal_image_layout(attachment_format: TextureFormat, usage: AttachmentUsage) -> ImageLayout {
    match usage {
        AttachmentUsage::Read => {
            if attachment_format.is_color() {
                return ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            }
            if attachment_format.is_depth() {
                return ImageLayout::DEPTH_READ_ONLY_OPTIMAL;
            }
            ImageLayout::DEPTH_ATTACHMENT_STENCIL_READ_ONLY_OPTIMAL
        }
        AttachmentUsage::Write => {
            if attachment_format.is_color() {
                return ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
            }
            if attachment_format.is_depth() {
                return ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
            }
            ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        }
        AttachmentUsage::ReadWrite => {
            if attachment_format.is_color() {
                return ImageLayout::GENERAL;
            }
            if attachment_format.is_depth() {
                return ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
            }
            ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderPass {
    handle: ash::vk::RenderPass,
    attachments: Vec<RenderPassAttachment>,
}
impl RenderPass {
    pub fn handle(&self) -> ash::vk::RenderPass {
        self.handle
    }

    pub fn attachments(&self) -> &Vec<RenderPassAttachment> {
        &self.attachments
    }
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderPassSync {
    pipeline_stage_from: PipelineStageFlags,
    access_flags_from: AccessFlags,
    pipeline_stage_to: PipelineStageFlags,
    access_flags_to: AccessFlags,
}

impl RenderPassSync {
    pub fn new(
        pipeline_stage_from: PipelineStageFlags,
        access_flags_from: AccessFlags,
        pipeline_stage_to: PipelineStageFlags,
        access_flags_to: AccessFlags,
    ) -> Self {
        Self {
            pipeline_stage_from,
            access_flags_from,
            pipeline_stage_to,
            access_flags_to,
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
    pub fn usage(&self, usage: AttachmentUsage) -> AccessFlags {
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

pub enum AttachmentUsage {
    Read,
    Write,
    ReadWrite,
}
