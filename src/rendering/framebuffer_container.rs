use std::{collections::HashMap, error::Error};

use ash::vk::{self, Framebuffer as F, FramebufferCreateInfo, ImageView};
use slotmap::SlotMap;

use crate::{
    device::DeviceContext,
    rendering::{
        pass_container::Pass,
        texture_container::{TextureContainer, TextureView, TextureViewId},
    },
};
#[derive(Default)]
pub struct FramebufferContainer {
    framebuffer_layout: SlotMap<FramebufferId, Vec<TextureViewId>>,
    framebuffers: HashMap<(FramebufferId, vk::RenderPass), Framebuffer>,
    /// To check if container already have framebuffer with given view ids
    views_to_id: HashMap<Vec<TextureViewId>, FramebufferId>,
    /// To invalidate ids that contain view id while deleting
    view_to_ids: HashMap<TextureViewId, Vec<FramebufferId>>,
    id_to_concrete: HashMap<FramebufferId, Vec<vk::RenderPass>>,
}
impl FramebufferContainer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn create_framebuffer(&mut self, texture_view_ids: Vec<TextureViewId>) -> FramebufferId {
        let id = self.framebuffer_layout.insert(texture_view_ids.clone());
        for view in texture_view_ids.iter() {
            let entry = self.view_to_ids.entry(*view).or_default();
            entry.push(id);
        }
        self.views_to_id.insert(texture_view_ids, id);
        id
    }
    pub(crate) fn get_concrete_framebuffer(
        &mut self,
        device: &DeviceContext,
        texture_container: &TextureContainer,
        framebuffer_id: FramebufferId,
        pass: &Pass,
    ) -> Result<&Framebuffer, Box<dyn Error>> {
        let pass_id = pass.render_pass().handle();
        if self.framebuffers.contains_key(&(framebuffer_id, pass_id)) {
            return Ok(self
                .framebuffers
                .get(&(framebuffer_id, pass_id))
                .expect("Unexpected"));
        }
        let texture_view_ids = self
            .framebuffer_layout
            .get(framebuffer_id)
            .ok_or_else(|| <Box<dyn Error>>::from("FramebufferId is invalid"))?;

        let framebuffer = create_framebuffer(
            device,
            texture_view_ids,
            pass.render_pass().handle(),
            texture_container,
        )?;
        _ = self
            .framebuffers
            .insert((framebuffer_id, pass_id), framebuffer);
        self.id_to_concrete
            .entry(framebuffer_id)
            .or_default()
            .push(pass_id);
        Ok(self
            .framebuffers
            .get(&(framebuffer_id, pass_id))
            .expect("Unexpected after insertion"))
    }
    pub fn get_concrete_framebuffer_with_render_pass(
        &mut self,
        device: &DeviceContext,
        texture_container: &TextureContainer,
        framebuffer_id: FramebufferId,
        render_pass: vk::RenderPass,
    ) -> Result<&Framebuffer, Box<dyn Error>> {
        let pass_id = render_pass;
        if self.framebuffers.contains_key(&(framebuffer_id, pass_id)) {
            return Ok(self
                .framebuffers
                .get(&(framebuffer_id, pass_id))
                .expect("Unexpected"));
        }
        let texture_view_ids = self
            .framebuffer_layout
            .get(framebuffer_id)
            .ok_or_else(|| <Box<dyn Error>>::from("FramebufferId is invalid"))?;

        let framebuffer =
            create_framebuffer(device, texture_view_ids, render_pass, texture_container)?;
        _ = self
            .framebuffers
            .insert((framebuffer_id, pass_id), framebuffer);
        self.id_to_concrete
            .entry(framebuffer_id)
            .or_default()
            .push(pass_id);
        Ok(self
            .framebuffers
            .get(&(framebuffer_id, pass_id))
            .expect("Unexpected after insertion"))
    }
    pub fn get_framebuffer_layout(&self, id: FramebufferId) -> Option<&Vec<TextureViewId>> {
        self.framebuffer_layout.get(id)
    }
    pub fn delete_image_view(&mut self, device: &DeviceContext, view: TextureViewId) {
        if let Some(ids) = self.view_to_ids.get(&view) {
            for id in ids {
                if let Some(rps) = self.id_to_concrete.get(id) {
                    for rp in rps.iter() {
                        {
                            let framebuffer = self
                                .framebuffers
                                .get(&(*id, *rp))
                                .expect("Unexpectedly framebuffer is not present");
                            unsafe { device.destroy_framebuffer(framebuffer.handle(), None) };
                            self.views_to_id.remove(&framebuffer.views_id);
                        }
                        self.framebuffers.remove(&(*id, *rp));
                    }
                }
                self.views_to_id = self
                    .views_to_id
                    .iter()
                    .filter(|&x| x.1 != id)
                    .map(|x| (x.0.clone(), *x.1))
                    .collect::<HashMap<Vec<TextureViewId>, FramebufferId>>();
                self.id_to_concrete.remove(id);
            }
        };
        self.view_to_ids.remove(&view);
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

    pub fn handle(&self) -> vk::Framebuffer {
        self.handle
    }
}
fn create_framebuffer(
    device: &DeviceContext,
    texture_view_ids: &[TextureViewId],
    pass: vk::RenderPass,
    texture_container: &TextureContainer,
) -> Result<Framebuffer, Box<dyn Error>> {
    let texture_views = texture_view_ids
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
        .render_pass(pass)
        .attachments(image_views.as_slice())
        .layers(1)
        .height(height)
        .width(width);
    let framebuffer = unsafe { device.create_framebuffer(&framebuffer_createinfo, None)? };
    Ok(Framebuffer {
        handle: framebuffer,
        width,
        height,
        views_id: texture_view_ids.to_vec(),
    })
}
