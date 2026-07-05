use std::collections::HashMap;

use crate::rendering::{
    buffer_container::GeneralBufferId, framebuffer_container::FramebufferId,
    texture_container::TextureId,
};

#[derive(Debug, Default)]
pub struct LabelContainer {
    labels: HashMap<LabelId, String>,
}
impl LabelContainer {
    pub fn insert_label(&mut self, id: LabelId, label: String) {
        self.labels.insert(id, label);
    }
    pub fn get_label(&self, id: &LabelId) -> Option<&str> {
        self.labels.get(id).map(|x| x.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LabelId {
    Texture(TextureId),
    Buffer(GeneralBufferId),
    Framebuffer(FramebufferId),
}
