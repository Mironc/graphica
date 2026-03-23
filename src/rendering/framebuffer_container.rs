use std::{collections::HashMap, error::Error};

use ash::vk::{Framebuffer as F, FramebufferCreateInfo, ImageView};
use slotmap::SlotMap;

use crate::{
    device::DeviceContext,
    rendering::{
        render_pass_container::RenderPass,
        texture_container::{TextureContainer, TextureView, TextureViewId},
    },
};
#[derive(Default)]
pub struct FramebufferContainer {
    framebuffers: SlotMap<FramebufferId, Framebuffer>,
    /// To check if container already have framebuffer with given view ids
    views_to_id: HashMap<Vec<TextureViewId>, FramebufferId>,
    /// To invalidate ids that contain view id while deleting
    view_to_ids: HashMap<TextureViewId, Vec<FramebufferId>>,
}
impl FramebufferContainer {
    pub fn new() -> Self {
        Self {
            framebuffers: SlotMap::default(),
            view_to_ids: HashMap::new(),
            views_to_id: HashMap::new(),
        }
    }
    pub fn insert_framebuffer(
        &mut self,
        device: &DeviceContext,
        texture_container: &TextureContainer,
        create: FramebufferCreate,
    ) -> Result<FramebufferId, Box<dyn Error>> {
        if let Some(id) = self.views_to_id.get(&create.views_ids) {
            return Ok(*id);
        }
        let texture_views = create
            .views_ids
            .iter()
            .map(|&x| texture_container.get_image_view(x))
            .collect::<Option<Vec<&TextureView>>>()
            .ok_or_else(|| <Box<dyn Error>>::from("Some TextureViewId is invalid"))?;
        let image_views = texture_views
            .iter()
            .map(|x| x.handle())
            .collect::<Vec<ImageView>>();

        let width = texture_views
            .iter()
            .min_by(|x, x1| x.extent().width.cmp(&x1.extent().width))
            .unwrap()
            .extent()
            .width;
        let height = texture_views
            .iter()
            .min_by(|x, x1| x.extent().height.cmp(&x1.extent().height))
            .unwrap()
            .extent()
            .height;

        let framebuffer_createinfo = FramebufferCreateInfo::default()
            .render_pass(create.render_pass.handle())
            .attachments(image_views.as_slice())
            .layers(1)
            .height(height)
            .width(width);
        let framebuffer = unsafe { device.create_framebuffer(&framebuffer_createinfo, None)? };

        let id = self.framebuffers.insert(Framebuffer {
            handle: framebuffer,
            width,
            height,
            views_id: create.views_ids.clone(),
        });
        for view in create.views_ids.iter() {
            let entry = self.view_to_ids.entry(*view).or_default();
            entry.push(id);
        }
        self.views_to_id.insert(create.views_ids, id);
        Ok(id)
    }
    pub fn get_framebuffer(&self, framebuffer_id: FramebufferId) -> Option<&Framebuffer> {
        self.framebuffers.get(framebuffer_id)
    }
    pub fn get_frambuffer_with_view(&self, view_ids: &Vec<TextureViewId>) -> Option<&Framebuffer> {
        self.get_framebuffer(*self.views_to_id.get(view_ids)?)
    }
    pub fn delete_image_view(&mut self, device: &DeviceContext, view: TextureViewId) {
        if let Some(ids) = self.view_to_ids.get(&view) {
            for &id in ids {
                if let Some(f) = self.framebuffers.get(id) {
                    unsafe { device.destroy_framebuffer(f.handle(), None) };
                }
                self.framebuffers.remove(id);
                self.views_to_id = self
                    .views_to_id
                    .iter()
                    .filter(|&x| *x.1 != id)
                    .map(|x| (x.0.clone(), *x.1))
                    .collect::<HashMap<Vec<TextureViewId>, FramebufferId>>();
            }
        };
    }
}

slotmap::new_key_type! {pub struct FramebufferId;}

pub struct Framebuffer {
    handle: F,
    height: u32,
    width: u32,
    views_id: Vec<TextureViewId>,
}

impl Framebuffer {
    pub fn views_id(&self) -> &[TextureViewId] {
        &self.views_id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn handle(&self) -> ash::vk::Framebuffer {
        self.handle
    }
}
pub struct FramebufferCreate<'a> {
    views_ids: Vec<TextureViewId>,
    render_pass: &'a RenderPass,
}
impl<'a> FramebufferCreate<'a> {
    pub fn new(views: Vec<TextureViewId>, render_pass: &'a RenderPass) -> Self {
        Self {
            views_ids: views,
            render_pass,
        }
    }
}
