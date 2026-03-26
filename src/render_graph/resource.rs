use crate::rendering::{buffer_container::GeneralBufferId, texture_container::TextureId};

///This struct is used to unify all ids
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceId {
    Texture(TextureId),
    Buffer(GeneralBufferId),
}
