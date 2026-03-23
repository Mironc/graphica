use crate::rendering::{
    buffer_container::{GeneralBuffer, GeneralBufferId},
    texture_container::{TextureId, TextureViewId},
};

///This struct is used to unify all ids
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceId {
    Texture(TextureId),
    Buffer(GeneralBufferId),
}
